use std::fmt::Debug;
use std::io::Error;
use std::pin::Pin;
use std::{io::Result, net::SocketAddr, task::Poll, time::Duration};

use futures::{AsyncRead, AsyncWrite, Future, Stream};

use crate::io::IoReactor;
use crate::ReactorHandle;

use super::sys::{self, Socket};
use super::Handle;

/// Tcp connection socket facade.
pub struct TcpConnection(Handle);

/// Convert tcp connection from [`SocketHandle`]
impl From<Handle> for TcpConnection {
    fn from(value: Handle) -> Self {
        Self(value)
    }
}

impl TcpConnection {
    /// Create new tcp client socket and return [`TcpConnect`] future.
    pub fn connect(
        reactor: IoReactor,
        remote: SocketAddr,
        bind_addr: Option<SocketAddr>,
        timeout: Option<Duration>,
    ) -> TcpConnect {
        match Self::client(reactor, remote, bind_addr) {
            Ok(handle) => TcpConnect {
                error: None,
                handle: Some(handle),
                remote,
                timeout,
            },
            Err(err) => TcpConnect {
                error: Some(err),
                handle: None,
                remote,
                timeout,
            },
        }
    }

    fn client(
        poller: IoReactor,
        remote: SocketAddr,
        bind_addr: Option<SocketAddr>,
    ) -> Result<Handle> {
        let socket = match remote {
            SocketAddr::V4(_) => Handle::tcp(true),
            SocketAddr::V6(_) => Handle::tcp(false),
        }?;

        if let Some(addr) = bind_addr {
            Handle::bind(socket, addr)?;
        }

        Handle::new(socket, poller)
    }

    /// Convert tcp connection to read stream
    pub fn to_read_stream<T: Into<Option<Duration>>>(&self, timeout: T) -> TcpConnectionReader {
        TcpConnectionReader {
            handle: self.0.clone(),
            timeout: timeout.into(),
        }
    }

    /// Convert tcp connection to write stream.
    pub fn to_write_stream<T: Into<Option<Duration>>>(&self, timeout: T) -> TcpConnectionWriter {
        TcpConnectionWriter {
            handle: self.0.clone(),
            timeout: timeout.into(),
        }
    }
}

/// Tcp connect future.
#[derive(Debug)]
pub struct TcpConnect {
    error: Option<Error>,
    handle: Option<Handle>,
    remote: SocketAddr,
    timeout: Option<Duration>,
}

impl Future for TcpConnect {
    type Output = Result<TcpConnection>;

    fn poll(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Self::Output> {
        if let Some(err) = self.error.take() {
            return Poll::Ready(Err(err));
        }

        let mut handle = self.handle.take().unwrap();

        let poll_connect =
            Pin::new(&mut handle).poll_connect(cx, self.remote, self.timeout.clone());

        match poll_connect {
            Poll::Pending => {
                self.handle = Some(handle);
                return Poll::Pending;
            }
            Poll::Ready(Ok(_)) => return Poll::Ready(Ok(TcpConnection(handle))),
            Poll::Ready(Err(err)) => {
                self.handle = Some(handle);

                return Poll::Ready(Err(err));
            }
        }
    }
}

/// Tcp connection read stream.
pub struct TcpConnectionReader {
    handle: Handle,
    timeout: Option<Duration>,
}

impl AsyncRead for TcpConnectionReader {
    fn poll_read(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut [u8],
    ) -> Poll<Result<usize>> {
        let timeout = self.timeout.clone();

        Pin::new(&mut self.handle).poll_read(cx, sys::ReadBuffer::Stream(buf), timeout)
    }
}

/// TcpConnection write stream
pub struct TcpConnectionWriter {
    handle: Handle,
    timeout: Option<Duration>,
}

impl AsyncWrite for TcpConnectionWriter {
    fn poll_close(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Result<()>> {
        Pin::new(&mut self.handle).poll_close(cx)
    }

    fn poll_flush(
        self: std::pin::Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
    ) -> Poll<Result<()>> {
        Poll::Ready(Ok(()))
    }

    fn poll_write(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> Poll<Result<usize>> {
        let timeout = self.timeout.clone();

        Pin::new(&mut self.handle).poll_write(cx, sys::WriteBuffer::Stream(buf), timeout)
    }
}

pub struct TcpAcceptor(Handle);

impl TcpAcceptor {
    /// Create new tcp listener with [`listen_addr`](SocketAddr)
    pub fn new(reactor: IoReactor, listen_addr: SocketAddr) -> Result<Self> {
        let handle = Handle::tcp(listen_addr.is_ipv4())?;

        Handle::listen(handle)?;

        Ok(Self(Handle::new(handle, reactor)?))
    }
}

impl Stream for TcpAcceptor {
    type Item = Result<(TcpConnection, SocketAddr)>;

    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Option<Self::Item>> {
        let mut handle = None;
        let mut remote = None;

        let poll = Pin::new(&mut self.0).poll_read(
            cx,
            sys::ReadBuffer::Accept(&mut handle, &mut remote),
            None,
        );

        match poll {
            Poll::Pending => Poll::Pending,
            Poll::Ready(Ok(_)) => {
                let handle =
                    handle.expect("Underlay accept returns success, but not set tcp handle");
                return Poll::Ready(Some(Ok((
                    TcpConnection::from(Handle::new(handle, self.0.reactor.clone())?),
                    remote.expect("Underlay accept returns success, but not set remote address"),
                ))));
            }
            Poll::Ready(Err(err)) => return Poll::Ready(Some(Err(err))),
        }
    }
}
