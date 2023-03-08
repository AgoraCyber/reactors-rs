use std::{io::Result, net::SocketAddr, task::Poll, time::Duration};

use futures::{Sink, Stream};

use crate::ReactorHandle;

use crate::io::poller::{PollerReactor, SysPoller};

use super::super::{SocketHandle, SocketReadBuffer, SocketWriteBuffer};

/// Udp socket facade.
pub struct UdpSocket<P>(SocketHandle<P>)
where
    P: SysPoller + Unpin + Clone + 'static;

/// Convert udp socket from [`SocketHandle`]
impl<P> From<SocketHandle<P>> for UdpSocket<P>
where
    P: SysPoller + Unpin + Clone + 'static,
{
    fn from(value: SocketHandle<P>) -> Self {
        Self(value)
    }
}

impl<P> UdpSocket<P>
where
    P: SysPoller + Unpin + Clone + 'static,
{
    /// Create new udp socket with [`listen_addr`](SocketAddr)
    pub fn new(poller: PollerReactor<P>, listen_addr: SocketAddr) -> Result<Self> {
        SocketHandle::udp(poller, listen_addr).map(|h| Self(h))
    }

    /// Convert udp socket to read stream
    pub fn to_read_stream<T: Into<Option<Duration>>>(
        &self,
        buff_size: usize,
        timeout: Option<Duration>,
    ) -> UdpSocketReader<P> {
        UdpSocketReader {
            handle: self.0.clone(),
            timeout: timeout.into(),
            buff_size,
        }
    }

    /// Convert udp socket to write stream.
    pub fn to_write_stream<T: Into<Option<Duration>>>(
        &self,
        timeout: Option<Duration>,
    ) -> UdpSocketWriter<P> {
        UdpSocketWriter {
            handle: self.0.clone(),
            timeout: timeout.into(),
            buff: None,
        }
    }
}

pub struct UdpSocketReader<P>
where
    P: SysPoller + Unpin + Clone + 'static,
{
    handle: SocketHandle<P>,
    timeout: Option<Duration>,
    buff_size: usize,
}

impl<P> Stream for UdpSocketReader<P>
where
    P: SysPoller + Unpin + Clone + 'static,
{
    type Item = Result<(Vec<u8>, SocketAddr)>;

    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        let mut buff = vec![0u8; self.buff_size];

        let mut remote = None;

        let timeout = self.timeout.clone();

        let read = self.handle.poll_read(
            SocketReadBuffer::Datagram(&mut buff, &mut remote),
            cx.waker().clone(),
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

pub struct UdpSocketWriter<P>
where
    P: SysPoller + Unpin + Clone + 'static,
{
    handle: SocketHandle<P>,
    timeout: Option<Duration>,
    buff: Option<(Vec<u8>, SocketAddr)>,
}

impl<P> Sink<(Vec<u8>, SocketAddr)> for UdpSocketWriter<P>
where
    P: SysPoller + Unpin + Clone + 'static,
{
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
            let write = self.handle.poll_write(
                SocketWriteBuffer::Datagram(&buff, &remote),
                cx.waker().clone(),
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
        self.handle.poll_close(cx.waker().clone())
    }
}
