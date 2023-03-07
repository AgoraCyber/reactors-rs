use crate::{
    io::poller::{PollerWrapper, SysPoller},
    ReactorHandle,
};

use super::{SocketReadBuffer, SocketWriteBuffer};

pub struct SocketHandle<P>(PollerWrapper<P>, i32)
where
    P: SysPoller + Clone + 'static;

#[allow(unused)]
impl<P> ReactorHandle for SocketHandle<P>
where
    P: SysPoller + Clone + 'static,
{
    type ReadBuffer<'cx> = SocketReadBuffer<'cx>;
    type WriteBuffer<'cx> = SocketWriteBuffer<'cx>;

    fn poll_close(&mut self) -> std::io::Result<()> {
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
