use std::{
    io::{Result, SeekFrom},
    path::PathBuf,
    sync::Arc,
    task::{Poll, Waker},
};

use futures::task::noop_waker;
use windows::Win32::Foundation::*;

use crate::{
    io::poller::{PollerReactor, SysPoller},
    ReactorHandle, ReactorHandleSeekable,
};

#[derive(Clone, Debug)]
pub struct FileHandle<P>
where
    P: SysPoller + Unpin + Clone + 'static,
{
    poller: PollerReactor<P>,
    fd: Arc<HANDLE>,
}

impl<P> Drop for FileHandle<P>
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

impl<P> FileHandle<P>
where
    P: SysPoller + Unpin + Clone + 'static,
{
    /// Create file handle with poller and posix [`FILE`](libc::FILE)
    pub fn new(poller: PollerReactor<P>, file: HANDLE) -> Self {
        Self {
            poller,
            fd: Arc::new(file),
        }
    }

    pub fn create(poller: PollerReactor<P>, path: PathBuf) -> Result<Self> {
        Self::fopen(poller, "w+", path.to_str().unwrap())
    }

    pub fn open(poller: PollerReactor<P>, path: PathBuf) -> Result<Self> {
        Self::fopen(poller, "a+", path.to_str().unwrap())
    }

    fn fopen(poller: PollerReactor<P>, mode: &str, path: &str) -> Result<Self> {
        unimplemented!()
    }
}

impl<P> ReactorHandle for FileHandle<P>
where
    P: SysPoller + Unpin + Clone + 'static,
{
    type ReadBuffer<'cx> = &'cx mut [u8];

    type WriteBuffer<'cx> = &'cx [u8];

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

impl<P> ReactorHandleSeekable for FileHandle<P>
where
    P: SysPoller + Unpin + Clone + 'static,
{
    fn seek(
        &mut self,
        pos: SeekFrom,
        _waker: Waker,
        _timeout: Option<std::time::Duration>,
    ) -> std::task::Poll<std::io::Result<u64>> {
        unimplemented!()
    }
}
