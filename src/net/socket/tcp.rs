use std::io::Error;
use std::mem::size_of;
use std::{io::Result, net::SocketAddr, os::fd::RawFd, task::Poll};

use errno::{errno, set_errno};
use futures::{future::BoxFuture, Future, FutureExt};
use os_socketaddr::OsSocketAddr;

use crate::reactor::Reactor;

use crate::net::reactor::{NetOpenOptions, NetReactor, ReadBuffer, WriteBuffer};

/// Extend [`NetReactor`] to add some tcp helper methods.
pub trait TcpSocketReactor {
    /// Tcp connection socket type
    type Socket: Unpin + 'static;

    /// Tcp listener socket type
    type ListenerSocket: Unpin + 'static;

    /// Future that returns by [`listen`](TcpSocketReactor::listen) method
    type Listen<'cx>: Future<Output = Result<Self::ListenerSocket>> + 'cx
    where
        Self: 'cx;

    /// Future that returns by [`connect`](TcpSocketReactor::connect) method
    type Connect<'cx>: Future<Output = Result<Self::Socket>> + 'cx
    where
        Self: 'cx;

    /// Start a new tcp server listener and bind to [`addr`](SocketAddr)
    fn listen<'a, 'cx>(&'a mut self, addr: SocketAddr) -> Self::Listen<'cx>
    where
        'a: 'cx;

    /// Create new tcp client and connect to remote endpoint.
    fn connect<'a, 'cx>(
        &'a mut self,
        to: SocketAddr,
        bind_addr: Option<SocketAddr>,
    ) -> Self::Connect<'cx>
    where
        'a: 'cx;
}

impl TcpSocketReactor for NetReactor {
    type Listen<'cx> = OpenTcpListener;

    type Connect<'cx> = OpenTcpSocket;

    type Socket = TcpSocket;

    type ListenerSocket = TcpListener;
    fn connect<'a, 'cx>(
        &'a mut self,
        to: SocketAddr,
        bind_addr: Option<SocketAddr>,
    ) -> Self::Connect<'cx>
    where
        'a: 'cx,
    {
        let opts = NetOpenOptions::TcpConnect {
            to,
            bind: bind_addr,
        };

        let fut = self.open(opts);

        OpenTcpSocket(self.clone(), fut, to)
    }

    fn listen<'a, 'cx>(&'a mut self, addr: SocketAddr) -> Self::Listen<'cx>
    where
        'a: 'cx,
    {
        let opts = NetOpenOptions::TcpListener(addr);

        let fut = self.open(opts);

        OpenTcpListener(self.clone(), fut, addr)
    }
}

/// Future to open tcp listener
pub struct OpenTcpListener(
    pub(crate) NetReactor,
    pub(crate) BoxFuture<'static, Result<RawFd>>,
    pub(crate) SocketAddr,
);

impl Future for OpenTcpListener {
    type Output = Result<TcpListener>;
    fn poll(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        match self.1.poll_unpin(cx) {
            Poll::Pending => Poll::Pending,

            Poll::Ready(Err(err)) => Poll::Ready(Err(err)),
            Poll::Ready(Ok(handle)) => Poll::Ready(Ok(TcpListener {
                reactor: self.0.clone(),
                fd: handle,
                bind_addr: self.2,
            })),
        }
    }
}

/// Future to open tcp connection
pub struct OpenTcpSocket(
    pub(crate) NetReactor,
    pub(crate) BoxFuture<'static, Result<RawFd>>,
    pub(crate) SocketAddr,
);

impl Future for OpenTcpSocket {
    type Output = Result<TcpSocket>;
    fn poll(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        match self.1.poll_unpin(cx) {
            Poll::Pending => Poll::Pending,

            Poll::Ready(Err(err)) => Poll::Ready(Err(err)),
            Poll::Ready(Ok(handle)) => Poll::Ready(Ok(TcpSocket {
                reactor: self.0.clone(),
                fd: handle,
                remote: self.2,
            })),
        }
    }
}

/// Socket instance with async io support
#[derive(Clone, Debug)]
pub struct TcpListener {
    reactor: NetReactor,
    fd: RawFd,
    bind_addr: SocketAddr,
}

impl TcpListener {
    /// Get connection peer [`address`](SocketAddr) or returns [`None`] if this socket is listener
    pub fn bind_addr(&self) -> &SocketAddr {
        &self.bind_addr
    }

    /// Accept next incoming connection.
    pub async fn accept(&mut self) -> Result<TcpSocket> {
        let mut remote = None;
        let mut handle = None;

        self.reactor
            .read(self.fd, ReadBuffer::Accept(&mut handle, &mut remote))
            .await?;

        Ok(TcpSocket {
            reactor: self.reactor.clone(),
            fd: handle.expect("Underlay accept returns success, but not set connection fd"),
            remote: remote.expect("Underlay accept returns success, but not set remote address"),
        })
    }
}

impl Drop for TcpListener {
    fn drop(&mut self) {
        log::debug!("close tcp socket({})", self.fd);
        self.reactor.close(self.fd).unwrap();
    }
}

/// Tcp connection socket.
#[derive(Clone, Debug)]
pub struct TcpSocket {
    reactor: NetReactor,
    fd: RawFd,
    remote: SocketAddr,
}

impl TcpSocket {
    /// Get connection peer [`address`](SocketAddr) or returns [`None`] if this socket is listener
    pub fn remote(&self) -> &SocketAddr {
        &self.remote
    }
}

impl Drop for TcpSocket {
    fn drop(&mut self) {
        log::debug!("close tcp socket({})", self.fd);
        self.reactor.close(self.fd).unwrap();
    }
}

impl futures::AsyncWrite for TcpSocket {
    fn poll_write(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<Result<usize>> {
        let handle = self.fd;
        let reactor = &mut self.reactor;

        let mut write = reactor.write(handle, WriteBuffer::Stream(buf));

        write.poll_unpin(cx)
    }

    fn poll_close(
        mut self: std::pin::Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<()>> {
        let handle = self.fd;
        let reactor = &mut self.reactor;

        Poll::Ready(reactor.close(handle))
    }

    fn poll_flush(
        self: std::pin::Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<()>> {
        Poll::Ready(Ok(()))
    }
}

impl futures::AsyncRead for TcpSocket {
    fn poll_read(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut [u8],
    ) -> std::task::Poll<Result<usize>> {
        let handle = self.fd;
        let reactor = &mut self.reactor;

        let mut read = reactor.read(handle, ReadBuffer::Stream(buf));

        read.poll_unpin(cx)
    }
}

pub struct OpenTcpConnect(
    pub(crate) NetReactor,
    pub(crate) Option<RawFd>,
    pub(crate) OsSocketAddr,
);

impl Future for OpenTcpConnect {
    type Output = Result<RawFd>;

    fn poll(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        log::debug!("socket({:?}) try to connect remote {:?}", self.1, self.2);

        use libc::*;

        let fd = self.1.unwrap();

        unsafe {
            let err_no: c_int = 0;

            let mut len = size_of::<c_int>() as u32;

            if libc::getsockopt(
                fd,
                libc::SOL_SOCKET,
                libc::SO_ERROR,
                err_no.to_be_bytes().as_mut_ptr() as *mut libc::c_void,
                &mut len as *mut u32,
            ) < 0
            {
                return Poll::Ready(Err(Error::last_os_error()));
            }

            if err_no != 0 {
                return Poll::Ready(Err(Error::from_raw_os_error(err_no)));
            }

            if libc::connect(fd, self.2.as_ptr(), self.2.len()) < 0 {
                let e = errno();

                set_errno(e);

                if e.0 == libc::EAGAIN || e.0 == libc::EWOULDBLOCK || e.0 == libc::EINPROGRESS {
                    // register event notify
                    self.0 .0.event_writable_set(fd, cx.waker().clone());

                    return Poll::Pending;
                } else if EISCONN == e.0 {
                    return Poll::Ready(Ok(self.1.take().unwrap()));
                } else {
                    return Poll::Ready(Err(Error::from_raw_os_error(e.0)));
                }
            } else {
                return Poll::Ready(Ok(self.1.take().unwrap()));
            }
        }
    }
}

impl Drop for OpenTcpConnect {
    fn drop(&mut self) {
        log::debug!("close tcp connect({:?})", self.1);
        if let Some(fd) = self.1.take() {
            unsafe {
                libc::close(fd);
            }
        }
    }
}
