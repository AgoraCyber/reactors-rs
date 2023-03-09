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
use windows::core::GUID;
use windows::Win32::Networking::WinSock::*;

use crate::{
    io::poller::{sys::PollerOVERLAPPED, PollRequest, PollerReactor, SysPoller},
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
    /// Create socket handle from raw socket fd.
    pub fn new(poller: PollerReactor<P>, fd: SOCKET) -> Self {
        Self {
            poller,
            fd: Arc::new(fd),
        }
    }
    /// Create udp socket with [`addr`](SocketAddr)
    pub fn udp(poller: PollerReactor<P>, addr: SocketAddr) -> Result<Self> {
        unsafe {
            let fd = match addr {
                SocketAddr::V4(_) => WSASocketW(
                    AF_INET.0 as i32,
                    SOCK_DGRAM as i32,
                    IPPROTO_UDP.0,
                    None,
                    0,
                    WSA_FLAG_OVERLAPPED,
                ),
                SocketAddr::V6(_) => WSASocketW(
                    AF_INET6.0 as i32,
                    SOCK_DGRAM as i32,
                    IPPROTO_UDP.0,
                    None,
                    0,
                    WSA_FLAG_OVERLAPPED,
                ),
            };

            if fd == INVALID_SOCKET {
                return Err(Error::last_os_error());
            }

            let addr: OsSocketAddr = addr.into();

            if bind(fd, (addr.as_ptr()).cast::<SOCKADDR>(), addr.len()) < 0 {
                return Err(Error::last_os_error());
            }

            Ok(Self::new(poller, fd))
        }
    }

    /// Create tcp socket
    pub fn tcp(poller: PollerReactor<P>, bind_addr: SocketAddr) -> Result<Self> {
        unsafe {
            let fd = match bind_addr {
                SocketAddr::V4(_) => WSASocketW(
                    AF_INET.0 as i32,
                    SOCK_STREAM as i32,
                    IPPROTO_TCP.0,
                    None,
                    0,
                    WSA_FLAG_OVERLAPPED,
                ),
                SocketAddr::V6(_) => WSASocketW(
                    AF_INET6.0 as i32,
                    SOCK_STREAM as i32,
                    IPPROTO_TCP.0,
                    None,
                    0,
                    WSA_FLAG_OVERLAPPED,
                ),
            };

            if fd == INVALID_SOCKET {
                return Err(Error::last_os_error());
            }

            let addr: OsSocketAddr = bind_addr.into();

            if bind(fd, (addr.as_ptr()).cast::<SOCKADDR>(), addr.len()) < 0 {
                return Err(Error::last_os_error());
            }

            Ok(Self::new(poller, fd))
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

        if let Some(overlapped) =
            self.poller
                .watch_writable_event_once(fd.0 as isize, cx.waker().clone(), timeout)?
        {}

        let connectex = self.get_connect_ex()?.unwrap();

        let addr = OsSocketAddr::from(remote);

        let overlapped = PollerOVERLAPPED::new(PollRequest::Writable(self.fd.0 as isize));

        unsafe {
            let overlapped = overlapped.into();
            if connectex(
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
                // release unused buff.
                _ = Box::<PollerOVERLAPPED>::from(overlapped);

                return Err(Error::last_os_error())?;
            }
        }

        // self.poller
        //     .watch_readable_event_once(fd.0 as isize, cx.waker().clone(), timeout)?;

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
        match buffer {
            SocketReadBuffer::Accept(handle, remote) => unsafe {
                if handle.is_none() {}
                // AcceptEx(
                //     slistensocket,
                //     sacceptsocket,
                //     lpoutputbuffer,
                //     dwreceivedatalength,
                //     dwlocaladdresslength,
                //     dwremoteaddresslength,
                //     lpdwbytesreceived,
                //     lpoverlapped,
                // )
            },
            SocketReadBuffer::Datagram(buff, remote) => {}
            SocketReadBuffer::Stream(buff) => {}
        }

        unimplemented!()
    }

    fn poll_write<'cx>(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buffer: Self::WriteBuffer<'cx>,
        timeout: Option<std::time::Duration>,
    ) -> std::task::Poll<std::io::Result<usize>> {
        unimplemented!()
    }
}
