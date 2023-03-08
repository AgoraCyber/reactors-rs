use std::io::Error;
use std::{io::Result, net::SocketAddr, task::Poll, time::Duration};

use futures::{AsyncRead, AsyncWrite, Future, Stream};

use crate::ReactorHandle;

use crate::io::poller::{PollerReactor, SysPoller};

use super::super::{SocketHandle, SocketReadBuffer, SocketWriteBuffer};

/// Tcp connection socket facade.
pub struct TcpConnection<P>(SocketHandle<P>)
where
    P: SysPoller + Unpin + Clone + 'static;

/// Convert tcp connection from [`SocketHandle`]
impl<P> From<SocketHandle<P>> for TcpConnection<P>
where
    P: SysPoller + Unpin + Clone + 'static,
{
    fn from(value: SocketHandle<P>) -> Self {
        Self(value)
    }
}

impl<P> TcpConnection<P>
where
    P: SysPoller + Unpin + Clone + 'static,
{
    pub fn connect(
        poller: PollerReactor<P>,
        remote: SocketAddr,
        bind_addr: Option<SocketAddr>,
    ) -> TcpConnect<P> {
        let bind_addr = if let Some(bind_addr) = bind_addr {
            bind_addr
        } else {
            "0.0.0.0:0".parse().unwrap()
        };

        match SocketHandle::tcp(poller, bind_addr) {
            Ok(handle) => TcpConnect {
                handle: Some(handle),
                remote,
                timeout: None,
                error: None,
            },
            Err(err) => TcpConnect {
                handle: None,
                remote,
                timeout: None,
                error: Some(err),
            },
        }
    }

    /// Convert tcp connection to read stream
    pub fn to_read_stream(&self, timeout: Option<Duration>) -> TcpConnectionReader<P> {
        TcpConnectionReader {
            handle: self.0.clone(),
            timeout,
        }
    }

    /// Convert tcp connection to write stream.
    pub fn to_write_stream(&self, timeout: Option<Duration>) -> TcpConnectionWriter<P> {
        TcpConnectionWriter {
            handle: self.0.clone(),
            timeout,
        }
    }
}

/// Tcp connect future.
pub struct TcpConnect<P>
where
    P: SysPoller + Unpin + Clone + 'static,
{
    error: Option<Error>,
    handle: Option<SocketHandle<P>>,
    remote: SocketAddr,
    timeout: Option<Duration>,
}

impl<P> Future for TcpConnect<P>
where
    P: SysPoller + Unpin + Clone + 'static,
{
    type Output = Result<TcpConnection<P>>;

    fn poll(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Self::Output> {
        if let Some(err) = self.error.take() {
            return Poll::Ready(Err(err));
        }

        let mut handle = self.handle.take().unwrap();

        let poll_connect =
            handle.poll_connect(self.remote, cx.waker().clone(), self.timeout.clone());

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
pub struct TcpConnectionReader<P>
where
    P: SysPoller + Unpin + Clone + 'static,
{
    handle: SocketHandle<P>,
    timeout: Option<Duration>,
}

impl<P> AsyncRead for TcpConnectionReader<P>
where
    P: SysPoller + Unpin + Clone + 'static,
{
    fn poll_read(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut [u8],
    ) -> Poll<Result<usize>> {
        let timeout = self.timeout.clone();

        self.handle
            .poll_read(SocketReadBuffer::Stream(buf), cx.waker().clone(), timeout)
    }
}

/// TcpConnection write stream
pub struct TcpConnectionWriter<P>
where
    P: SysPoller + Unpin + Clone + 'static,
{
    handle: SocketHandle<P>,
    timeout: Option<Duration>,
}

impl<P> AsyncWrite for TcpConnectionWriter<P>
where
    P: SysPoller + Unpin + Clone + 'static,
{
    fn poll_close(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Result<()>> {
        self.handle.poll_close(cx.waker().clone())
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

        self.handle
            .poll_write(SocketWriteBuffer::Stream(buf), cx.waker().clone(), timeout)
    }
}

pub struct TcpAcceptor<P>(SocketHandle<P>)
where
    P: SysPoller + Unpin + Clone + 'static;

impl<P> TcpAcceptor<P>
where
    P: SysPoller + Unpin + Clone + 'static,
{
    /// Create new tcp listener with [`listen_addr`](SocketAddr)
    pub fn new(poller: PollerReactor<P>, listen_addr: SocketAddr) -> Result<Self> {
        SocketHandle::tcp(poller, listen_addr).map(|h| Self(h))
    }
}

impl<P> Stream for TcpAcceptor<P>
where
    P: SysPoller + Unpin + Clone + 'static,
{
    type Item = Result<(TcpConnection<P>, SocketAddr)>;

    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Option<Self::Item>> {
        let mut handle = None;
        let mut remote = None;

        let poll = self.0.poll_read(
            SocketReadBuffer::Accept(&mut handle, &mut remote),
            cx.waker().clone(),
            None,
        );

        match poll {
            Poll::Pending => Poll::Pending,
            Poll::Ready(Ok(_)) => {
                return Poll::Ready(Some(Ok((
                    TcpConnection::from(
                        handle.expect("Underlay accept returns success, but not set tcp handle"),
                    ),
                    remote.expect("Underlay accept returns success, but not set remote address"),
                ))))
            }
            Poll::Ready(Err(err)) => return Poll::Ready(Some(Err(err))),
        }
    }
}
