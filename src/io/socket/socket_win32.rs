use std::{
    ffi::c_void,
    io::{Error, Result},
    mem::size_of,
    net::SocketAddr,
    pin::Pin,
    ptr::null_mut,
    sync::Arc,
    task::{Context, Poll},
};

use futures::task::noop_waker_ref;

use os_socketaddr::OsSocketAddr;
use windows::core::GUID;
use windows::Win32::Networking::WinSock::*;

use crate::{
    io::poller::{PollerReactor, SysPoller},
    ReactorHandle,
};

use super::{SocketReadBuffer, SocketWriteBuffer};

static WSAID_CONNECTEX: GUID = GUID::from_values(
    0x25a207b9,
    0xddf3,
    0x4660,
    [0x8e, 0xe9, 0x76, 0xe5, 0x8c, 0x74, 0x06, 0x3e],
);

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
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        remote: SocketAddr,
        timeout: Option<std::time::Duration>,
    ) -> std::task::Poll<std::io::Result<()>> {
        unsafe {
            let connectex = Some(null_mut());
            let mut bytes_returned = 0u32;
            if WSAIoctl(
                *self.fd,
                SIO_GET_EXTENSION_FUNCTION_POINTER,
                Some((&WSAID_CONNECTEX as *const GUID).cast::<c_void>()),
                size_of::<GUID>() as u32,
                connectex,
                size_of::<*mut c_void>() as u32,
                &mut bytes_returned as *mut u32,
                None,
                None,
            ) != 0
            {
                return Poll::Ready(Err(Error::last_os_error()));
            }
        }

        unimplemented!()
    }
}

impl<P> ReactorHandle for SocketHandle<P>
where
    P: SysPoller + Unpin + Clone + 'static,
{
    type ReadBuffer<'cx> = SocketReadBuffer<'cx, P>;
    type WriteBuffer<'cx> = SocketWriteBuffer<'cx>;

    fn poll_close(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Result<()>> {
        unimplemented!()
    }

    fn poll_read<'cx>(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buffer: Self::ReadBuffer<'cx>,

        timeout: Option<std::time::Duration>,
    ) -> std::task::Poll<std::io::Result<usize>> {
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
