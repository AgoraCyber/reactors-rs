use std::{
    io::*,
    net::SocketAddr,
    ptr::null_mut,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    task::Poll,
    time::Duration,
};

use os_socketaddr::OsSocketAddr;
use winapi::{shared::ws2def::*, um::ioapiset::*, um::winsock2::*};

use crate::{
    io::{Poller, RawFd},
    Reactor, ReactorHandle,
};

use super::sys::{self, Socket};

/// Socket handle wrapper.
#[derive(Debug, Clone)]
pub struct Handle<R>
where
    R: Reactor + Poller + Unpin + Clone + 'static,
{
    /// Socket handle bind reactor
    pub reactor: R,
    /// Socket handle bind os fd.
    pub fd: Arc<SOCKET>,

    pub closed: Arc<AtomicBool>,
}

impl<R> Handle<R>
where
    R: Reactor + Poller + Unpin + Clone + 'static,
{
    fn to_raw_fd(&self) -> RawFd {
        *self.fd as RawFd
    }
}

impl<R> Drop for Handle<R>
where
    R: Reactor + Poller + Unpin + Clone + 'static,
{
    fn drop(&mut self) {
        // Only self alive.
        if Arc::strong_count(&self.fd) == 1 {
            self.close();
        }
    }
}

impl<R> sys::Socket for Handle<R>
where
    R: Reactor + Poller + Unpin + Clone + 'static,
{
    type Reactor = R;

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

    fn new(fd: RawFd, reactor: Self::Reactor) -> Result<Self> {
        // bind fd to completion port.
        unsafe {
            let completion_port = reactor.io_handle();

            let ret = CreateIoCompletionPort(fd, completion_port, 0, 0);

            if ret == null_mut() {
                return Err(Error::last_os_error());
            }
        }

        Ok(Self {
            reactor,
            fd: Arc::new(fd as usize),
            closed: Default::default(),
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

        // Check if previous io event is ready.
        // if let Some(result) = self.reactor.poll_io_event(fd)? {}

        unimplemented!()
    }
}

#[allow(unused)]
impl<R> ReactorHandle for Handle<R>
where
    R: Reactor + Poller + Unpin + Clone + 'static,
{
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
        unimplemented!()
    }
}
