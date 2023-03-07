#[cfg_attr(target_family = "unix", path = "socket_posix.rs")]
#[cfg_attr(target_family = "windows", path = "socket_win32.rs")]
mod impls;

use std::{io::Result, net::SocketAddr, task::Poll, time::Duration};

use futures::{AsyncRead, AsyncWrite, Sink, Stream};
pub use impls::*;

use crate::ReactorHandle;

use super::poller::SysPoller;

/// Socket [`ReadBuffer`](crate::reactor::ReactorHandle::ReadBuffer)
pub enum SocketReadBuffer<'cx, P>
where
    P: SysPoller + Unpin + Clone + 'static,
{
    Stream(&'cx mut [u8]),
    Datagram(&'cx mut [u8], &'cx mut Option<SocketAddr>),

    Accept(
        &'cx mut Option<SocketHandle<P>>,
        &'cx mut Option<SocketAddr>,
    ),
}

/// Socket [`WriteBuffer`](crate::reactor::ReactorHandle::WriteBuffer)
pub enum SocketWriteBuffer<'cx> {
    Stream(&'cx [u8]),
    Datagram(&'cx [u8], &'cx SocketAddr),
}

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
    /// Convert udp socket to read stream
    pub fn to_read_stream(
        &self,
        buff_size: usize,
        timeout: Option<Duration>,
    ) -> UdpSocketReader<P> {
        UdpSocketReader {
            handle: self.0.clone(),
            timeout,
            buff_size,
        }
    }

    /// Convert udp socket to write stream.
    pub fn to_write_stream(&self, timeout: Option<Duration>) -> UdpSocketWriter<P> {
        UdpSocketWriter {
            handle: self.0.clone(),
            timeout,
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
