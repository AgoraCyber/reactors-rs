use std::pin::Pin;
use std::{io::Result, net::SocketAddr, task::Poll, time::Duration};

use futures::{Sink, Stream};

use super::sys::{ReadBuffer, Socket, WriteBuffer};
use super::Handle;
use crate::io::IoReactor;
use crate::ReactorHandle;

/// Udp socket facade.
pub struct UdpSocket(Handle);

/// Convert udp socket from [`Handle`]
impl From<Handle> for UdpSocket {
    fn from(value: Handle) -> Self {
        Self(value)
    }
}

impl UdpSocket {
    /// Create new udp socket with [`listen_addr`](SocketAddr)
    pub fn new(reactor: IoReactor, listen_addr: SocketAddr) -> Result<Self> {
        let fd = Handle::udp(listen_addr.is_ipv4())?;

        Handle::bind(fd, listen_addr)?;

        Ok(Self(Handle::new(listen_addr.is_ipv4(), fd, reactor)?))
    }

    /// Convert udp socket to read stream
    pub fn to_read_stream<T: Into<Option<Duration>>>(
        &self,
        buff_size: usize,
        timeout: T,
    ) -> UdpSocketReader {
        UdpSocketReader {
            handle: self.0.clone(),
            timeout: timeout.into(),
            buff_size,
        }
    }

    /// Convert udp socket to write stream.
    pub fn to_write_stream<T: Into<Option<Duration>>>(&self, timeout: T) -> UdpSocketWriter {
        UdpSocketWriter {
            handle: self.0.clone(),
            timeout: timeout.into(),
            buff: None,
        }
    }
}

pub struct UdpSocketReader {
    handle: Handle,
    timeout: Option<Duration>,
    buff_size: usize,
}

impl Stream for UdpSocketReader {
    type Item = Result<(Vec<u8>, SocketAddr)>;

    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        let mut buff = vec![0u8; self.buff_size];

        let mut remote = None;

        let timeout = self.timeout.clone();

        let read = Pin::new(&mut self.handle).poll_read(
            cx,
            ReadBuffer::Datagram(&mut buff, &mut remote),
            timeout,
        );

        match read {
            Poll::Pending => Poll::Pending,
            Poll::Ready(Ok(len)) => Poll::Ready(Some(Ok((
                buff[0..len].to_vec(),
                remote.expect("Underlay implement recvfrom success but not set remote address"),
            )))),
            Poll::Ready(Err(err)) => Poll::Ready(Some(Err(err))),
        }
    }
}

pub struct UdpSocketWriter {
    handle: Handle,
    timeout: Option<Duration>,
    buff: Option<(Vec<u8>, SocketAddr)>,
}

impl Sink<(Vec<u8>, SocketAddr)> for UdpSocketWriter {
    type Error = std::io::Error;

    fn poll_ready(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<std::result::Result<(), Self::Error>> {
        if self.buff.is_some() {
            return self.poll_flush(cx);
        }

        Poll::Ready(Ok(()))
    }

    fn start_send(
        mut self: std::pin::Pin<&mut Self>,
        item: (Vec<u8>, SocketAddr),
    ) -> std::result::Result<(), Self::Error> {
        self.buff = Some(item);

        Ok(())
    }

    fn poll_flush(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<std::result::Result<(), Self::Error>> {
        let send_buff = self.buff.take();

        let timeout = self.timeout.clone();

        if let Some((buff, remote)) = send_buff {
            let write = Pin::new(&mut self.handle).poll_write(
                cx,
                WriteBuffer::Datagram(&buff, &remote),
                timeout,
            );

            match write {
                Poll::Ready(result) => match result {
                    Ok(_) => Poll::Ready(Ok(())),
                    Err(err) => Poll::Ready(Err(err)),
                },
                Poll::Pending => {
                    self.buff = Some((buff, remote));
                    Poll::Pending
                }
            }
        } else {
            Poll::Ready(Ok(()))
        }
    }
    fn poll_close(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<std::result::Result<(), Self::Error>> {
        Pin::new(&mut self.handle).poll_close(cx)
    }
}

#[cfg(test)]
mod tests {
    use std::{task::Poll, time::Duration};

    use futures::{FutureExt, SinkExt, TryStreamExt};
    use futures_test::task::noop_context;

    use crate::{io::IoReactor, Reactor};

    use super::UdpSocket;

    #[futures_test::test]
    async fn test_udp() {
        _ = pretty_env_logger::try_init();

        let mut reactor = IoReactor::default();

        let server_addr = "127.0.0.1:1812".parse().unwrap();

        let client_addr = "127.0.0.1:1813".parse().unwrap();

        let server = UdpSocket::new(reactor.clone(), server_addr).unwrap();

        let client = UdpSocket::new(reactor.clone(), client_addr).unwrap();

        let mut server_write_stream = server.to_write_stream(None);
        let mut server_read_stream = server.to_read_stream(1024, None);

        let mut client_write_stream = client.to_write_stream(None);
        let mut client_read_stream = client.to_read_stream(1024, None);

        let mut send = client_write_stream.send((b"hello server".to_vec(), server_addr));

        loop {
            match send.poll_unpin(&mut noop_context()) {
                Poll::Pending => {
                    reactor.poll_once(Duration::from_secs(1)).unwrap();
                }
                Poll::Ready(result) => {
                    result.unwrap();
                    break;
                }
            }
        }

        let mut try_next = server_read_stream.try_next();

        loop {
            match try_next.poll_unpin(&mut noop_context()) {
                Poll::Pending => {
                    reactor.poll_once(Duration::from_secs(1)).unwrap();
                }
                Poll::Ready(result) => {
                    let (buff, remote) = result.unwrap().unwrap();

                    assert_eq!(b"hello server".to_vec(), buff);

                    assert_eq!(remote, client_addr);
                    break;
                }
            }
        }

        // server to client

        let mut send = server_write_stream.send((b"hello client".to_vec(), client_addr));

        loop {
            match send.poll_unpin(&mut noop_context()) {
                Poll::Pending => {
                    reactor.poll_once(Duration::from_secs(1)).unwrap();
                }
                Poll::Ready(result) => {
                    result.unwrap();
                    break;
                }
            }
        }

        let mut try_next = client_read_stream.try_next();

        loop {
            match try_next.poll_unpin(&mut noop_context()) {
                Poll::Pending => {
                    reactor.poll_once(Duration::from_secs(1)).unwrap();
                }
                Poll::Ready(result) => {
                    let (buff, remote) = result.unwrap().unwrap();

                    assert_eq!(b"hello client".to_vec(), buff);

                    assert_eq!(remote, server_addr);
                    break;
                }
            }
        }
    }
}
