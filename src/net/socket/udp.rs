use std::{io::Result, net::SocketAddr, task::Poll};

use futures::{future::BoxFuture, Future, FutureExt, Sink, Stream};

use crate::reactor::Reactor;

use crate::net::reactor::{NetOpenOptions, NetReactor, ReadBuffer, SocketFd, WriteBuffer};

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
    pub(crate) BoxFuture<'static, Result<SocketFd>>,
    pub(crate) usize,
);

impl Future for UdpSocketOpen {
    type Output = Result<UdpSocket>;
    fn poll(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        match self.1.poll_unpin(cx) {
            Poll::Pending => Poll::Pending,

            Poll::Ready(Err(err)) => Poll::Ready(Err(err)),
            Poll::Ready(Ok(handle)) => Poll::Ready(Ok((self.0.clone(), handle, self.2).into())),
        }
    }
}

/// Udp socket instance with async io support
#[derive(Clone, Debug)]
pub struct UdpSocket {
    reactor: NetReactor,
    handle: SocketFd,
    buff_size: usize,
    send_buff: Option<(Vec<u8>, SocketAddr)>,
}

impl From<(NetReactor, SocketFd, usize)> for UdpSocket {
    fn from(value: (NetReactor, SocketFd, usize)) -> Self {
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
        log::debug!("close udp socket({:?})", self.handle);
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
        let opts = NetOpenOptions::Udp(bind_addr);

        let fut = self.open(opts);

        UdpSocketOpen(self.clone(), fut, buff_size)
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
