use std::{io::Result, marker::PhantomData, net::SocketAddr, os::fd::RawFd, task::Poll};

use futures::{Future, FutureExt, Sink, Stream};

use crate::reactor::Reactor;

use super::reactor::{NetOpenOptions, NetReactor, ReadBuffer, WriteBuffer};

pub trait TcpSocketReactor {
    type Socket: Unpin + 'static;

    type Listen<'cx>: Future<Output = Result<Self::Socket>> + 'cx
    where
        Self: 'cx;

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
    type Listen<'cx> = SocketOpen<TcpSocket>;

    type Connect<'cx> = SocketOpen<TcpSocket>;

    type Socket = TcpSocket;
    fn connect<'a, 'cx>(
        &'a mut self,
        to: SocketAddr,
        bind_addr: Option<SocketAddr>,
    ) -> Self::Connect<'cx>
    where
        'a: 'cx,
    {
        SocketOpen::<TcpSocket>(
            self.clone(),
            NetOpenOptions::TcpConnect {
                to,
                bind: bind_addr,
            },
            Default::default(),
        )
    }

    fn listen<'a, 'cx>(&'a mut self, addr: SocketAddr) -> Self::Connect<'cx>
    where
        'a: 'cx,
    {
        SocketOpen::<TcpSocket>(
            self.clone(),
            NetOpenOptions::TcpListener(addr),
            Default::default(),
        )
    }
}

/// Future for tcp socket open .
pub struct SocketOpen<Socket: From<(NetReactor, RawFd)> + Unpin>(
    pub(crate) NetReactor,
    pub(crate) NetOpenOptions,
    pub(crate) PhantomData<Socket>,
);

impl<Socket: From<(NetReactor, RawFd)> + Unpin> Future for SocketOpen<Socket> {
    type Output = Result<Socket>;
    fn poll(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        let ops = self.1.clone();
        let reactor = &mut self.0;

        let mut open = reactor.open(ops);

        match open.poll_unpin(cx) {
            Poll::Pending => Poll::Pending,

            Poll::Ready(Err(err)) => Poll::Ready(Err(err)),
            Poll::Ready(Ok(handle)) => Poll::Ready(Ok((reactor.clone(), handle).into())),
        }
    }
}

/// Socket instance with async io support
#[derive(Clone, Debug)]
pub struct TcpSocket {
    reactor: NetReactor,
    fd: RawFd,
    remote: Option<SocketAddr>,
}

impl From<(NetReactor, RawFd)> for TcpSocket {
    fn from(value: (NetReactor, RawFd)) -> Self {
        Self {
            reactor: value.0,
            fd: value.1,
            remote: Default::default(),
        }
    }
}

impl TcpSocket {
    /// Get connection peer [`address`](SocketAddr) or returns [`None`] if this socket is listener
    pub fn remote(&self) -> &Option<SocketAddr> {
        &self.remote
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
            remote: Some(
                remote.expect("Underlay accept returns success, but not set remote address"),
            ),
        })
    }
}

impl Drop for TcpSocket {
    fn drop(&mut self) {
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

/// Extend [`NetReactor`] to add some udp helper methods.
pub trait UdpSocketReactor {
    type Socket: Unpin + 'static;

    type Open<'cx>: Future<Output = Result<Self::Socket>> + 'cx
    where
        Self: 'cx;

    /// Create new tcp client and connect to remote endpoint.
    fn udp<'a, 'cx>(&'a mut self, bind_addr: SocketAddr, buff_size: usize) -> Self::Open<'cx>
    where
        'a: 'cx;
}

/// Future for tcp socket open .
pub struct UdpSocketOpen(
    pub(crate) NetReactor,
    pub(crate) NetOpenOptions,
    pub(crate) usize,
);

impl Future for UdpSocketOpen {
    type Output = Result<UdpSocket>;
    fn poll(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        let ops = self.1.clone();
        let reactor = &mut self.0;

        let mut open = reactor.open(ops);

        match open.poll_unpin(cx) {
            Poll::Pending => Poll::Pending,

            Poll::Ready(Err(err)) => Poll::Ready(Err(err)),
            Poll::Ready(Ok(handle)) => Poll::Ready(Ok((reactor.clone(), handle, self.2).into())),
        }
    }
}

/// Udp socket instance with async io support
#[derive(Clone, Debug)]
pub struct UdpSocket {
    reactor: NetReactor,
    handle: RawFd,
    buff_size: usize,
    send_buff: Option<(Vec<u8>, SocketAddr)>,
}

impl From<(NetReactor, RawFd, usize)> for UdpSocket {
    fn from(value: (NetReactor, RawFd, usize)) -> Self {
        Self {
            reactor: value.0,
            handle: value.1,
            buff_size: value.2,
            send_buff: Default::default(),
        }
    }
}

impl Drop for UdpSocket {
    fn drop(&mut self) {
        _ = self.reactor.close(self.handle);
    }
}

impl UdpSocketReactor for NetReactor {
    type Socket = UdpSocket;
    type Open<'cx> = UdpSocketOpen;

    fn udp<'a, 'cx>(&'a mut self, bind_addr: SocketAddr, buff_size: usize) -> Self::Open<'cx>
    where
        'a: 'cx,
    {
        UdpSocketOpen(self.clone(), NetOpenOptions::Udp(bind_addr), buff_size)
    }
}

impl Stream for UdpSocket {
    type Item = Result<(Vec<u8>, SocketAddr)>;
    fn poll_next(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Option<Self::Item>> {
        let handle = self.handle;

        let mut reactor = self.reactor.clone();

        let mut buff = vec![0u8; self.buff_size];

        let mut remote = None;

        let mut read = reactor.read(handle, ReadBuffer::Datagram(&mut buff, &mut remote));

        match read.poll_unpin(cx) {
            Poll::Ready(result) => match result {
                Ok(len) => Poll::Ready(Some(Ok((
                    buff[0..len].to_vec(),
                    remote.expect("Underlay implement recvfrom success but not set remote address"),
                )))),
                Err(err) => Poll::Ready(Some(Err(err))),
            },
            Poll::Pending => Poll::Pending,
        }
    }
}

impl Sink<(Vec<u8>, SocketAddr)> for UdpSocket {
    type Error = std::io::Error;

    fn poll_ready(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<std::result::Result<(), Self::Error>> {
        if self.send_buff.is_some() {
            return self.poll_flush(cx);
        }

        Poll::Ready(Ok(()))
    }

    fn start_send(
        mut self: std::pin::Pin<&mut Self>,
        item: (Vec<u8>, SocketAddr),
    ) -> std::result::Result<(), Self::Error> {
        self.send_buff = Some(item);

        Ok(())
    }

    fn poll_flush(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<std::result::Result<(), Self::Error>> {
        let send_buff = self.send_buff.take();

        if let Some((buff, remote)) = send_buff {
            let handle = self.handle;

            let mut reactor = self.reactor.clone();

            let mut write = reactor.write(handle, WriteBuffer::Datagram(&buff, remote.clone()));

            match write.poll_unpin(cx) {
                Poll::Ready(result) => match result {
                    Ok(_) => Poll::Ready(Ok(())),
                    Err(err) => Poll::Ready(Err(err)),
                },
                Poll::Pending => {
                    self.send_buff = Some((buff, remote));
                    Poll::Pending
                }
            }
        } else {
            Poll::Ready(Ok(()))
        }
    }
    fn poll_close(
        self: std::pin::Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
    ) -> Poll<std::result::Result<(), Self::Error>> {
        let handle = self.handle;

        let mut reactor = self.reactor.clone();

        Poll::Ready(reactor.close(handle))
    }
}
