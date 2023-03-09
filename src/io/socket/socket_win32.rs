use std::{
    ffi::c_void,
    io::{Error, Result},
    mem::{size_of, transmute},
    net::SocketAddr,
    pin::Pin,
    ptr::{null, null_mut},
    sync::Arc,
    task::{Context, Poll},
};

use futures::task::noop_waker_ref;

use once_cell::sync::OnceCell;
use os_socketaddr::OsSocketAddr;
use winapi::shared::winerror::ERROR_IO_PENDING;
use windows::Win32::Foundation::*;
use windows::Win32::Networking::WinSock::*;
use windows::{core::*, Win32::System::IO::CreateIoCompletionPort};

use crate::{
    io::poller::{sys::PollerOVERLAPPED, PollContext, PollRequest, PollerReactor, SysPoller},
    ReactorHandle,
};

use super::{SocketReadBuffer, SocketWriteBuffer};

static WSAID_CONNECTEX: GUID = GUID::from_values(
    0x25a207b9,
    0xddf3,
    0x4660,
    [0x8e, 0xe9, 0x76, 0xe5, 0x8c, 0x74, 0x06, 0x3e],
);

// static WSAID_DISCONNECTEX: GUID = GUID::from_values(
//     0x7fda2e11,
//     0x8630,
//     0x436f,
//     [0xa0, 0x31, 0xf5, 0x36, 0xa6, 0xee, 0xc1, 0x57],
// );

#[derive(Clone, Debug)]
pub struct SocketHandle<P>
where
    P: SysPoller + Unpin + Clone + 'static,
{
    poller: PollerReactor<P>,
    fd: Arc<SOCKET>,
    v4: bool,
}

impl<P> Drop for SocketHandle<P>
where
    P: SysPoller + Unpin + Clone + 'static,
{
    fn drop(&mut self) {
        // Only self
        if Arc::strong_count(&self.fd) == 1 {
            _ = Pin::new(self).poll_close(&mut Context::from_waker(noop_waker_ref()));
        }
    }
}

impl<P> SocketHandle<P>
where
    P: SysPoller + Unpin + Clone + 'static,
{
    fn socket(iocp_handle: HANDLE, r#type: u16, protocol: IPPROTO, v4: bool) -> Result<SOCKET> {
        let socket = unsafe {
            match v4 {
                true => WSASocketW(
                    AF_INET.0 as i32,
                    r#type as i32,
                    protocol.0,
                    None,
                    0,
                    WSA_FLAG_OVERLAPPED,
                ),
                false => WSASocketW(
                    AF_INET6.0 as i32,
                    r#type as i32,
                    protocol.0,
                    None,
                    0,
                    WSA_FLAG_OVERLAPPED,
                ),
            }
        };

        if socket == INVALID_SOCKET {
            return Err(Error::last_os_error());
        }

        unsafe {
            // bind socket to completion port.
            CreateIoCompletionPort(HANDLE(socket.0 as isize), iocp_handle, 0, 0)?;
        }

        Ok(socket)
    }

    /// Create socket handle from raw socket fd.
    pub fn new(poller: PollerReactor<P>, fd: SOCKET, v4: bool) -> Self {
        Self {
            poller,
            fd: Arc::new(fd),
            v4,
        }
    }
    /// Create udp socket with [`addr`](SocketAddr)
    pub fn udp(poller: PollerReactor<P>, addr: SocketAddr) -> Result<Self> {
        unsafe {
            let (fd, v4) = match addr {
                SocketAddr::V4(_) => (
                    Self::socket(poller.iocp_handle(), SOCK_DGRAM, IPPROTO_UDP, true)?,
                    true,
                ),
                SocketAddr::V6(_) => (
                    Self::socket(poller.iocp_handle(), SOCK_DGRAM, IPPROTO_UDP, false)?,
                    false,
                ),
            };

            if fd == INVALID_SOCKET {
                return Err(Error::last_os_error());
            }

            let addr: OsSocketAddr = addr.into();

            if bind(fd, (addr.as_ptr()).cast::<SOCKADDR>(), addr.len()) < 0 {
                return Err(Error::last_os_error());
            }

            Ok(Self::new(poller, fd, v4))
        }
    }

    /// Create tcp socket
    pub fn tcp(poller: PollerReactor<P>, bind_addr: SocketAddr) -> Result<Self> {
        unsafe {
            let (fd, v4) = match bind_addr {
                SocketAddr::V4(_) => (
                    Self::socket(poller.iocp_handle(), SOCK_STREAM, IPPROTO_TCP, true)?,
                    true,
                ),
                SocketAddr::V6(_) => (
                    Self::socket(poller.iocp_handle(), SOCK_STREAM, IPPROTO_TCP, false)?,
                    false,
                ),
            };

            if fd == INVALID_SOCKET {
                return Err(Error::last_os_error());
            }

            let addr: OsSocketAddr = bind_addr.into();

            if bind(fd, (addr.as_ptr()).cast::<SOCKADDR>(), addr.len()) < 0 {
                return Err(Error::last_os_error());
            }

            Ok(Self::new(poller, fd, v4))
        }
    }

    /// Tcep acceptor socket start listening incoming connection
    pub fn listen(&mut self) -> Result<()> {
        unsafe {
            if listen(*self.fd, SOMAXCONN as i32) < 0 {
                return Err(Error::last_os_error());
            } else {
                Ok(())
            }
        }
    }
    pub fn poll_connect(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        remote: SocketAddr,
        timeout: Option<std::time::Duration>,
    ) -> std::task::Poll<std::io::Result<()>> {
        let fd = *self.fd;

        if let Some(context) = self.poller.poll_event(fd.0 as isize)? {
            match context {
                PollContext::Connect => return Poll::Ready(Ok(())),
                _ => {
                    panic!("expect PollContext::Connect, but got {:?}", context);
                }
            }
        }

        let connectex = self.get_connect_ex()?.unwrap();

        let addr = OsSocketAddr::from(remote);

        let overlapped = PollerOVERLAPPED::new(
            PollRequest::Writable(self.fd.0 as isize),
            PollContext::Connect,
        );

        unsafe {
            let overlapped = overlapped.into();
            if !connectex(
                fd,
                addr.as_ptr() as *const SOCKADDR,
                addr.len(),
                null_mut(),
                0,
                null_mut(),
                overlapped,
            )
            .as_bool()
            {
                if WSAGetLastError().0 as u32 != ERROR_IO_PENDING {
                    // release unused buff.
                    _ = Box::<PollerOVERLAPPED>::from(overlapped);

                    return Err(Error::last_os_error())?;
                }
            } else {
                // connected
                _ = Box::<PollerOVERLAPPED>::from(overlapped);

                return Poll::Ready(Ok(()));
            }
        }

        self.poller.on_event(
            PollRequest::Writable(fd.0 as isize),
            cx.waker().clone(),
            timeout,
        );

        Poll::Pending
    }

    fn get_connect_ex(&self) -> Result<&'static LPFN_CONNECTEX> {
        static CONNECT_EX: OnceCell<LPFN_CONNECTEX> = OnceCell::new();

        let fd = *self.fd;

        CONNECT_EX.get_or_try_init(|| unsafe {
            let connectex: *const c_void = null();
            let mut bytes_returned = 0u32;
            if WSAIoctl(
                fd,
                SIO_GET_EXTENSION_FUNCTION_POINTER,
                Some((&WSAID_CONNECTEX as *const GUID).cast::<c_void>()),
                size_of::<GUID>() as u32,
                Some(transmute(&connectex)),
                size_of::<*mut c_void>() as u32,
                &mut bytes_returned as *mut u32,
                None,
                None,
            ) == SOCKET_ERROR
            {
                return Err(Error::last_os_error());
            }

            Ok(transmute(connectex))
        })
    }
}

impl<P> ReactorHandle for SocketHandle<P>
where
    P: SysPoller + Unpin + Clone + 'static,
{
    type ReadBuffer<'cx> = SocketReadBuffer<'cx, P>;
    type WriteBuffer<'cx> = SocketWriteBuffer<'cx>;

    fn poll_close(
        self: std::pin::Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
    ) -> Poll<Result<()>> {
        unsafe {
            if closesocket(*self.fd) == SOCKET_ERROR {
                return Poll::Ready(Err(Error::last_os_error()));
            } else {
                return Poll::Ready(Ok(()));
            }
        }
    }

    fn poll_read<'cx>(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buffer: Self::ReadBuffer<'cx>,

        timeout: Option<std::time::Duration>,
    ) -> std::task::Poll<std::io::Result<usize>> {
        let fd = *self.fd;

        match buffer {
            SocketReadBuffer::Accept(handle, remote) => unsafe {
                if let Some(context) = self.poller.poll_event(fd.0 as isize)? {
                    match context {
                        PollContext::Accept(fd, addr) => {
                            *handle = Some(SocketHandle::new(
                                self.poller.clone(),
                                SOCKET(fd as usize),
                                self.v4,
                            ));

                            let sock_len = if self.v4 {
                                size_of::<winapi::shared::ws2def::SOCKADDR_IN>()
                            } else {
                                size_of::<winapi::shared::ws2ipdef::SOCKADDR_IN6>()
                            };

                            let addr = OsSocketAddr::copy_from_raw(
                                addr[0..16]
                                    .as_ptr()
                                    .cast::<winapi::shared::ws2def::SOCKADDR>(),
                                sock_len as i32,
                            );

                            *remote = addr.into();

                            return Poll::Ready(Ok(0));
                        }
                        _ => {
                            panic!("expect PollContext::Connect, but got {:?}", context);
                        }
                    }
                }
                let fd = *self.fd;
                let accept_socket =
                    Self::socket(self.poller.iocp_handle(), SOCK_STREAM, IPPROTO_TCP, self.v4)?;

                let buff = Box::into_raw(Box::new([0u8, 32])) as *mut c_void;

                let overlapped = PollerOVERLAPPED::new(
                    PollRequest::Readable(self.fd.0 as isize),
                    PollContext::Accept(
                        accept_socket.0 as isize,
                        Box::from_raw(buff as *mut [u8; 32]),
                    ),
                );

                let overlapped = overlapped.into();

                if !AcceptEx(fd, accept_socket, buff, 0, 16, 16, null_mut(), overlapped).as_bool() {
                    if WSAGetLastError().0 as u32 != ERROR_IO_PENDING {
                        _ = Box::<PollerOVERLAPPED>::from(overlapped);

                        return Err(Error::last_os_error())?;
                    }
                } else {
                    *handle = Some(SocketHandle::new(
                        self.poller.clone(),
                        accept_socket,
                        self.v4,
                    ));

                    let buff = Box::from_raw(buff as *mut [u8; 32]);

                    let sock_len = if self.v4 {
                        size_of::<winapi::shared::ws2def::SOCKADDR_IN>()
                    } else {
                        size_of::<winapi::shared::ws2ipdef::SOCKADDR_IN6>()
                    };

                    let addr = OsSocketAddr::copy_from_raw(
                        buff[0..16]
                            .as_ptr()
                            .cast::<winapi::shared::ws2def::SOCKADDR>(),
                        sock_len as i32,
                    );

                    *remote = addr.into();

                    Box::into_raw(buff);

                    _ = Box::<PollerOVERLAPPED>::from(overlapped);

                    return Poll::Ready(Ok(0));
                }
            },
            SocketReadBuffer::Datagram(buff, remote) => {
                if let Some(context) = self.poller.poll_event(fd.0 as isize)? {
                    match context {
                        PollContext::RecvFrom(len, addr) => {
                            let sock_len = if self.v4 {
                                size_of::<winapi::shared::ws2def::SOCKADDR_IN>()
                            } else {
                                size_of::<winapi::shared::ws2ipdef::SOCKADDR_IN6>()
                            };

                            unsafe {
                                let addr = OsSocketAddr::copy_from_raw(
                                    addr.as_ptr().cast::<winapi::shared::ws2def::SOCKADDR>(),
                                    sock_len as i32,
                                );

                                *remote = addr.into();
                            }

                            return Poll::Ready(Ok(len));
                        }
                        _ => {
                            panic!("expect PollContext::Connect, but got {:?}", context);
                        }
                    }
                }

                let buffers = [WSABUF {
                    buf: PSTR(buff.as_ptr() as *mut u8),
                    len: buff.len() as u32,
                }];

                let mut recv_bytes = 0u32;

                let addr_buff = Box::into_raw(Box::new([0u8, 16])) as *mut SOCKADDR;

                let mut addr_buff_len = 0i32;

                let overlapped = PollerOVERLAPPED::new(
                    PollRequest::Readable(fd.0 as isize),
                    PollContext::RecvFrom(0, unsafe { Box::from_raw(addr_buff as *mut [u8; 16]) }),
                );

                let overlapped = Some(overlapped.into());

                unsafe {
                    if WSARecvFrom(
                        fd,
                        &buffers,
                        Some(&mut recv_bytes as *mut u32),
                        null_mut(),
                        Some(addr_buff),
                        Some(&mut addr_buff_len as *mut i32),
                        overlapped,
                        None,
                    ) != 0
                    {
                        if WSAGetLastError().0 as u32 != ERROR_IO_PENDING {
                            _ = Box::<PollerOVERLAPPED>::from(overlapped.unwrap());

                            return Err(Error::last_os_error())?;
                        }
                    } else {
                        let buff = Box::from_raw(addr_buff as *mut [u8; 32]);

                        let addr = OsSocketAddr::copy_from_raw(
                            buff[0..addr_buff_len as usize]
                                .as_ptr()
                                .cast::<winapi::shared::ws2def::SOCKADDR>(),
                            addr_buff_len,
                        );

                        *remote = addr.into();

                        Box::into_raw(buff);

                        _ = Box::<PollerOVERLAPPED>::from(overlapped.unwrap());

                        return Poll::Ready(Ok(recv_bytes as usize));
                    }
                }
            }
            SocketReadBuffer::Stream(buff) => {
                if let Some(context) = self.poller.poll_event(fd.0 as isize)? {
                    match context {
                        PollContext::Read(len) => {
                            return Poll::Ready(Ok(len));
                        }
                        _ => {
                            panic!("expect PollContext::Connect, but got {:?}", context);
                        }
                    }
                }

                let buffers = [WSABUF {
                    buf: PSTR(buff.as_ptr() as *mut u8),
                    len: buff.len() as u32,
                }];

                let mut recv_bytes = 0u32;

                let overlapped = PollerOVERLAPPED::new(
                    PollRequest::Readable(fd.0 as isize),
                    PollContext::Read(0),
                );

                let overlapped = Some(overlapped.into());

                unsafe {
                    if WSARecv(
                        fd,
                        &buffers,
                        Some(&mut recv_bytes as *mut u32),
                        null_mut(),
                        overlapped,
                        None,
                    ) != 0
                    {
                        if WSAGetLastError().0 as u32 != ERROR_IO_PENDING {
                            _ = Box::<PollerOVERLAPPED>::from(overlapped.unwrap());

                            return Err(Error::last_os_error())?;
                        }
                    } else {
                        _ = Box::<PollerOVERLAPPED>::from(overlapped.unwrap());

                        return Poll::Ready(Ok(recv_bytes as usize));
                    }
                }
            }
        }

        self.poller.on_event(
            PollRequest::Readable(fd.0 as isize),
            cx.waker().clone(),
            timeout,
        );

        Poll::Pending
    }

    fn poll_write<'cx>(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buffer: Self::WriteBuffer<'cx>,
        timeout: Option<std::time::Duration>,
    ) -> std::task::Poll<std::io::Result<usize>> {
        let fd = *self.fd;

        match buffer {
            SocketWriteBuffer::Datagram(buff, remote) => {
                if let Some(context) = self.poller.poll_event(fd.0 as isize)? {
                    match context {
                        PollContext::SendTo(len) => {
                            return Poll::Ready(Ok(len));
                        }
                        _ => {
                            panic!("expect PollContext::Connect, but got {:?}", context);
                        }
                    }
                }

                let buffers = [WSABUF {
                    buf: PSTR(buff.as_ptr() as *mut u8),
                    len: buff.len() as u32,
                }];

                let mut send_bytes = 0u32;

                let remote = OsSocketAddr::from(remote.clone());

                let overlapped = PollerOVERLAPPED::new(
                    PollRequest::Writable(fd.0 as isize),
                    PollContext::SendTo(0),
                );

                let overlapped = Some(overlapped.into());

                unsafe {
                    if WSASendTo(
                        fd,
                        &buffers,
                        Some(&mut send_bytes as *mut u32),
                        0,
                        Some(remote.as_ptr() as *const SOCKADDR),
                        remote.len(),
                        overlapped,
                        None,
                    ) != 0
                    {
                        if WSAGetLastError().0 as u32 != ERROR_IO_PENDING {
                            _ = Box::<PollerOVERLAPPED>::from(overlapped.unwrap());

                            return Err(Error::last_os_error())?;
                        }
                    } else {
                        _ = Box::<PollerOVERLAPPED>::from(overlapped.unwrap());

                        return Poll::Ready(Ok(send_bytes as usize));
                    }
                }
            }
            SocketWriteBuffer::Stream(buff) => {
                if let Some(context) = self.poller.poll_event(fd.0 as isize)? {
                    match context {
                        PollContext::Write(len) => {
                            return Poll::Ready(Ok(len));
                        }
                        _ => {
                            panic!("expect PollContext::Connect, but got {:?}", context);
                        }
                    }
                }

                let buffers = [WSABUF {
                    buf: PSTR(buff.as_ptr() as *mut u8),
                    len: buff.len() as u32,
                }];

                let mut send_bytes = 0u32;

                let overlapped = PollerOVERLAPPED::new(
                    PollRequest::Writable(fd.0 as isize),
                    PollContext::Write(0),
                );

                let overlapped = Some(overlapped.into());

                unsafe {
                    if WSASend(
                        fd,
                        &buffers,
                        Some(&mut send_bytes as *mut u32),
                        0,
                        overlapped,
                        None,
                    ) != 0
                    {
                        if WSAGetLastError().0 as u32 != ERROR_IO_PENDING {
                            _ = Box::<PollerOVERLAPPED>::from(overlapped.unwrap());

                            return Err(Error::last_os_error())?;
                        }
                    } else {
                        _ = Box::<PollerOVERLAPPED>::from(overlapped.unwrap());

                        return Poll::Ready(Ok(send_bytes as usize));
                    }
                }
            }
        }

        self.poller.on_event(
            PollRequest::Writable(fd.0 as isize),
            cx.waker().clone(),
            timeout,
        );

        Poll::Pending
    }
}
