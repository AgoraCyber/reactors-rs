use std::{
    io::Result,
    net::SocketAddr,
    sync::Arc,
    task::{Poll, Waker},
};

use futures::task::noop_waker;
use windows::Win32::Networking::WinSock::*;

use crate::{
    io::poller::{PollerReactor, SysPoller},
    ReactorHandle,
};

use super::{SocketReadBuffer, SocketWriteBuffer};

#[derive(Clone, Debug)]
pub struct SocketHandle<P>
where
    P: SysPoller + Unpin + Clone + 'static,
{
    poller: PollerReactor<P>,
    fd: Arc<SOCKET>,
}

impl<P> Drop for SocketHandle<P>
where
    P: SysPoller + Unpin + Clone + 'static,
{
    fn drop(&mut self) {
        // Only self
        if Arc::strong_count(&self.fd) == 1 {
            _ = self.poll_close(noop_waker());
        }
    }
}

impl<P> SocketHandle<P>
where
    P: SysPoller + Unpin + Clone + 'static,
{
    /// Create socket handle from raw socket fd.
    pub fn new(poller: PollerReactor<P>, fd: SOCKET) -> Self {
        Self {
            poller,
            fd: Arc::new(fd),
        }
    }
    /// Create udp socket with [`addr`](SocketAddr)
    pub fn udp(poller: PollerReactor<P>, addr: SocketAddr) -> Result<Self> {
        unimplemented!()
    }

    /// Create tcp socket
    pub fn tcp(poller: PollerReactor<P>, bind_addr: SocketAddr) -> Result<Self> {
        unimplemented!()
    }

    /// Tcep acceptor socket start listening incoming connection
    pub fn listen(&mut self) -> Result<()> {
        unimplemented!()
    }
    pub fn poll_connect(
        &mut self,
        remote: SocketAddr,
        waker: std::task::Waker,
        timeout: Option<std::time::Duration>,
    ) -> std::task::Poll<std::io::Result<()>> {
        unimplemented!()
    }
}

impl<P> ReactorHandle for SocketHandle<P>
where
    P: SysPoller + Unpin + Clone + 'static,
{
    type ReadBuffer<'cx> = SocketReadBuffer<'cx, P>;
    type WriteBuffer<'cx> = SocketWriteBuffer<'cx>;

    fn poll_close(&mut self, _waker: Waker) -> Poll<Result<()>> {
        unimplemented!()
    }

    fn poll_read<'cx>(
        &mut self,
        buffer: Self::ReadBuffer<'cx>,
        waker: std::task::Waker,
        timeout: Option<std::time::Duration>,
    ) -> std::task::Poll<std::io::Result<usize>> {
        unimplemented!()
    }

    fn poll_write<'cx>(
        &mut self,
        buffer: Self::WriteBuffer<'cx>,
        waker: std::task::Waker,
        timeout: Option<std::time::Duration>,
    ) -> std::task::Poll<std::io::Result<usize>> {
        unimplemented!()
    }
}
