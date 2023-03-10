use std::{
    ffi::c_void,
    io::*,
    mem::{size_of, transmute},
    net::SocketAddr,
    ptr::{null, null_mut},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    task::Poll,
    time::Duration,
};

use once_cell::sync::OnceCell;
use os_socketaddr::OsSocketAddr;
use winapi::{
    shared::{guiddef::*, winerror::ERROR_IO_PENDING, ws2def::*},
    um::ioapiset::*,
    um::{errhandlingapi::GetLastError, winsock2::*},
    um::{minwinbase::OVERLAPPED, mswsock::*},
};

use crate::{
    io::{EventMessage, EventName, IoReactor, RawFd, ReactorOverlapped},
    ReactorHandle,
};

use super::sys::{self, ReadBuffer, Socket};

/// Socket handle wrapper.
#[derive(Debug, Clone)]
pub struct Handle {
    /// Socket handle bind reactor
    pub reactor: IoReactor,
    /// Socket handle bind os fd.
    pub fd: Arc<SOCKET>,
    /// If this socket is ipv4 familiy
    pub ip_v4: bool,
    /// Close status
    pub closed: Arc<AtomicBool>,
}

impl Handle {
    fn to_raw_fd(&self) -> RawFd {
        *self.fd as RawFd
    }
}

impl Drop for Handle {
    fn drop(&mut self) {
        // Only self alive.
        if Arc::strong_count(&self.fd) == 1 {
            self.close();
        }
    }
}

impl sys::Socket for Handle {
    fn bind(fd: RawFd, addr: std::net::SocketAddr) -> Result<()> {
        unsafe {
            let addr: OsSocketAddr = addr.into();

            if bind(fd as usize, (addr.as_ptr()).cast::<SOCKADDR>(), addr.len()) < 0 {
                return Err(Error::last_os_error());
            }
        }

        Ok(())
    }

    fn listen(fd: RawFd) -> Result<()> {
        unsafe {
            if listen(fd as usize, SOMAXCONN as i32) < 0 {
                return Err(Error::last_os_error());
            } else {
                Ok(())
            }
        }
    }

    fn new(ip_v4: bool, fd: RawFd, reactor: IoReactor) -> Result<Self> {
        // bind fd to completion port.
        unsafe {
            let completion_port = reactor.io_handle();

            let ret = CreateIoCompletionPort(fd, completion_port, 0, 0);

            if ret == null_mut() {
                // release socket resource when this method raise an error.
                closesocket(fd as usize);
                return Err(Error::last_os_error());
            }
        }

        Ok(Self {
            reactor,
            fd: Arc::new(fd as usize),
            closed: Default::default(),
            ip_v4,
        })
    }

    fn socket(ip_v4: bool, sock_type: i32, protocol: i32) -> Result<RawFd> {
        let socket = unsafe {
            match ip_v4 {
                true => WSASocketW(
                    AF_INET as i32,
                    sock_type as i32,
                    protocol as i32,
                    null_mut(),
                    0,
                    WSA_FLAG_OVERLAPPED,
                ),
                false => WSASocketW(
                    AF_INET6 as i32,
                    sock_type as i32,
                    protocol as i32,
                    null_mut(),
                    0,
                    WSA_FLAG_OVERLAPPED,
                ),
            }
        };

        if socket == INVALID_SOCKET {
            return Err(Error::last_os_error());
        }

        Ok(socket as RawFd)
    }

    fn close(&mut self) {
        unsafe {
            closesocket(*self.fd);
        }
    }

    fn tcp(ip_v4: bool) -> Result<RawFd> {
        Self::socket(
            ip_v4,
            winapi::shared::ws2def::SOCK_STREAM,
            IPPROTO_TCP as i32,
        )
    }

    fn udp(ip_v4: bool) -> Result<RawFd> {
        Self::socket(
            ip_v4,
            winapi::shared::ws2def::SOCK_DGRAM,
            IPPROTO_UDP as i32,
        )
    }

    /// Start an async connect operator.
    #[allow(unused)]
    fn poll_connect(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        remote: SocketAddr,
        timeout: Option<Duration>,
    ) -> Poll<Result<()>> {
        // socket fd
        let fd = self.to_raw_fd();

        if let Some(event) = self.reactor.poll_io_event(fd, EventName::Connect)? {
            match event.message? {
                EventMessage::Connect => {
                    return Poll::Ready(Ok(()));
                }
                _ => {
                    panic!("Inner error")
                }
            }
        }

        let overlapped = ReactorOverlapped::new_raw(fd, EventName::Connect);

        #[allow(non_snake_case)]
        let ConnectEx = self.get_connect_ex()?.unwrap();

        let addr: OsSocketAddr = remote.into();

        let ret = unsafe {
            ConnectEx(
                fd as usize,
                addr.as_ptr() as *const SOCKADDR,
                addr.len(),
                null_mut(),
                0,
                null_mut(),
                overlapped as *mut OVERLAPPED,
            )
        };

        log::trace!("socket({:?}) connect({})", fd, ret);

        if ret > 0 {
            // obtain point ownership
            let overlapped: Box<ReactorOverlapped> = overlapped.into();

            return Poll::Ready(Ok(()));
        }

        // This operation will completing Asynchronously
        if unsafe { GetLastError() } == ERROR_IO_PENDING {
            log::trace!("socket({:?}) connect asynchronously", fd);

            self.reactor
                .once(fd, EventName::Connect, cx.waker().clone(), timeout);

            return Poll::Pending;
        }

        return Poll::Ready(Err(Error::last_os_error()));
    }
}

#[allow(unused)]
impl ReactorHandle for Handle {
    type ReadBuffer<'cx> = sys::ReadBuffer<'cx>;
    type WriteBuffer<'cx> = sys::WriteBuffer<'cx>;

    fn poll_write<'cx>(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buffer: Self::WriteBuffer<'cx>,
        timeout: Option<std::time::Duration>,
    ) -> std::task::Poll<Result<usize>> {
        unimplemented!()
    }

    fn poll_close(
        mut self: std::pin::Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<()>> {
        match self
            .closed
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        {
            Err(_) => Poll::Ready(Ok(())),
            _ => {
                let fd = self.to_raw_fd();
                // cancel all pending future.
                self.reactor.cancel_all(fd);

                Poll::Ready(Ok(()))
            }
        }
    }

    fn poll_read<'cx>(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buffer: Self::ReadBuffer<'cx>,
        timeout: Option<std::time::Duration>,
    ) -> std::task::Poll<Result<usize>> {
        match buffer {
            ReadBuffer::Accept(fd, remote) => self.poll_accept(cx, fd, remote, timeout),
            ReadBuffer::Datagram(buff, remote) => {
                self.poll_read_datagram(cx, buff, remote, timeout)
            }
            ReadBuffer::Stream(buff) => self.poll_read_stream(cx, buff, timeout),
        }
    }
}

impl Handle {
    fn get_connect_ex(&self) -> Result<&'static LPFN_CONNECTEX> {
        static CONNECT_EX: OnceCell<LPFN_CONNECTEX> = OnceCell::new();

        let fd = self.to_raw_fd();

        CONNECT_EX.get_or_try_init(|| unsafe {
            let connectex: *const c_void = null();
            let mut bytes_returned = 0u32;
            if WSAIoctl(
                fd as usize,
                SIO_GET_EXTENSION_FUNCTION_POINTER,
                transmute(&WSAID_CONNECTEX),
                size_of::<GUID>() as u32,
                transmute(&connectex),
                size_of::<*mut c_void>() as u32,
                &mut bytes_returned as *mut u32,
                null_mut(),
                None,
            ) == SOCKET_ERROR
            {
                return Err(Error::last_os_error());
            }

            Ok(transmute(connectex))
        })
    }
    fn poll_accept<'cx>(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        conn_fd: &'cx mut Option<RawFd>,
        remote: &'cx mut Option<SocketAddr>,
        timeout: Option<std::time::Duration>,
    ) -> std::task::Poll<Result<usize>> {
        let fd = self.to_raw_fd();

        if let Some(event) = self.reactor.poll_io_event(fd, EventName::Accept)? {
            match event.message? {
                EventMessage::Accept(fd, addr) => {
                    *remote = addr;
                    *conn_fd = Some(fd);

                    return Poll::Ready(Ok(0));
                }
                _ => {
                    panic!("Inner error")
                }
            }
        }

        let accept_socket = Self::tcp(self.ip_v4)?;

        let overlapped = ReactorOverlapped::new_raw(fd, EventName::Accept);

        let mut bytes_received = 0u32;

        unsafe {
            (*overlapped).accept_fd = accept_socket;

            let addr_len = (*overlapped).addrs.len() / 2;

            let ret = AcceptEx(
                fd as usize,
                accept_socket as usize,
                (*overlapped).addrs.as_mut_ptr() as *mut c_void,
                0,
                addr_len as u32,
                addr_len as u32,
                &mut bytes_received,
                overlapped as *mut OVERLAPPED,
            );

            log::trace!("socket({:?}) accept({})", fd, ret);

            if ret > 0 {
                // obtain point ownership
                let overlapped: Box<ReactorOverlapped> = overlapped.into();

                let remote_addr = OsSocketAddr::copy_from_raw(
                    overlapped.addrs[16..].as_ptr() as *const SOCKADDR,
                    16,
                );

                *remote = remote_addr.into();
                *conn_fd = Some(accept_socket);

                return Poll::Ready(Ok(0));
            }

            // This operation will completing Asynchronously
            if GetLastError() == ERROR_IO_PENDING {
                log::trace!("socket({:?}) accept asynchronously", fd);

                self.reactor
                    .once(fd, EventName::Accept, cx.waker().clone(), timeout);

                return Poll::Pending;
            }

            return Poll::Ready(Err(Error::last_os_error()));
        }
    }

    fn poll_read_datagram<'cx>(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buff: &'cx mut [u8],
        remote: &'cx mut Option<SocketAddr>,
        timeout: Option<std::time::Duration>,
    ) -> std::task::Poll<Result<usize>> {
        unimplemented!()
    }

    fn poll_read_stream<'cx>(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buff: &'cx mut [u8],
        timeout: Option<std::time::Duration>,
    ) -> std::task::Poll<Result<usize>> {
        unimplemented!()
    }
}
