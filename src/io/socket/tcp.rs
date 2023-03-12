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
        } else {
            let bind_addr = if remote.is_ipv4() {
                "0.0.0.0:0".parse().expect("random bind address for ipv4")
            } else {
                "[::]:0".parse().expect("random bind address for ipv6")
            };

            Handle::bind(socket, bind_addr)?;
        }

        Handle::new(remote.is_ipv4(), socket, poller)
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

pub struct TcpAcceptor(Handle, Option<IoReactor>);

impl TcpAcceptor {
    /// Create new tcp listener with [`listen_addr`](SocketAddr)
    ///
    /// If `connection_reactor` is not [`None`],
    /// the incoming connections will bind to that [`reactor`](IoReactor) instance
    pub fn new(
        reactor: IoReactor,
        listen_addr: SocketAddr,
        connection_reactor: Option<IoReactor>,
    ) -> Result<Self> {
        let handle = Handle::tcp(listen_addr.is_ipv4())?;

        Handle::bind(handle, listen_addr)?;

        Handle::listen(handle)?;

        Ok(Self(
            Handle::new(listen_addr.is_ipv4(), handle, reactor)?,
            connection_reactor,
        ))
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

                // bind incoming connection to another io reactor instance.
                let reactor = if let Some(connection_reactor) = &self.1 {
                    connection_reactor.clone()
                } else {
                    self.0.reactor.clone()
                };

                return Poll::Ready(Some(Ok((
                    TcpConnection::from(Handle::new(self.0.ip_v4, handle, reactor)?),
                    remote.expect("Underlay accept returns success, but not set remote address"),
                ))));
            }
            Poll::Ready(Err(err)) => return Poll::Ready(Some(Err(err))),
        }
    }
}

#[cfg(test)]
mod tests {

    use futures::{AsyncReadExt, AsyncWriteExt, FutureExt, TryStreamExt};
    use futures_test::task::noop_context;

    use crate::{io::IoReactor, Reactor};

    use super::*;

    #[futures_test::test]
    async fn test_acceptor() {
        _ = pretty_env_logger::try_init();

        let mut reactor = IoReactor::default();

        let listen_addr = "127.0.0.1:1812".parse().unwrap();

        let mut acceptor = TcpAcceptor::new(reactor.clone(), listen_addr, None).unwrap();

        reactor.poll_once(Duration::from_secs(1)).unwrap();

        // assert_stream_pending!(acceptor);

        let mut connect = TcpConnection::connect(reactor.clone(), listen_addr, None, None);

        let client_connection: TcpConnection;

        // try connect
        loop {
            match connect.poll_unpin(&mut noop_context()) {
                Poll::Pending => {
                    reactor.poll_once(Duration::from_secs(1)).unwrap();
                }
                Poll::Ready(result) => {
                    client_connection = result.unwrap();
                    break;
                }
            }
        }

        let mut try_next = acceptor.try_next();

        let server_connection: TcpConnection;

        // Accept one
        loop {
            match try_next.poll_unpin(&mut noop_context()) {
                Poll::Pending => {
                    reactor.poll_once(Duration::from_secs(1)).unwrap();
                }
                Poll::Ready(result) => {
                    (server_connection, _) = result.unwrap().unwrap();
                    break;
                }
            }
        }

        let mut write_stream = client_connection.to_write_stream(None);

        let mut write = write_stream.write(&b"hello world"[..]);

        loop {
            match write.poll_unpin(&mut noop_context()) {
                Poll::Pending => {
                    reactor.poll_once(Duration::from_secs(1)).unwrap();
                }
                Poll::Ready(result) => {
                    assert_eq!(result.unwrap(), 11);
                    break;
                }
            }
        }

        let mut read_stream = server_connection.to_read_stream(None);

        let mut buff = [0u8; 32];

        let mut read = read_stream.read(&mut buff);

        loop {
            match read.poll_unpin(&mut noop_context()) {
                Poll::Pending => {
                    reactor.poll_once(Duration::from_secs(1)).unwrap();
                }
                Poll::Ready(result) => {
                    assert_eq!(result.unwrap(), 11);
                    assert_eq!(&buff[..11], b"hello world");
                    break;
                }
            }
        }
    }
}
