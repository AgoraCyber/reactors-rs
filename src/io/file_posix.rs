use std::{io::SeekFrom, task::Waker};

use crate::{
    io::poller::{PollerWrapper, SysPoller},
    ReactorHandle, ReactorHandleSeekable,
};

pub struct FileHandle<P>(PollerWrapper<P>, *mut libc::FILE)
where
    P: SysPoller + Unpin + Clone + 'static;

#[allow(unused)]
impl<P> ReactorHandle for FileHandle<P>
where
    P: SysPoller + Unpin + Clone + 'static,
{
    type ReadBuffer<'cx> = &'cx mut [u8];

    type WriteBuffer<'cx> = &'cx [u8];

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

#[allow(unused)]
impl<P> ReactorHandleSeekable for FileHandle<P>
where
    P: SysPoller + Unpin + Clone + 'static,
{
    fn seek(
        &mut self,
        pos: SeekFrom,
        waker: Waker,
        timeout: Option<std::time::Duration>,
    ) -> std::task::Poll<std::io::Result<u64>> {
        unimplemented!()
    }
}
