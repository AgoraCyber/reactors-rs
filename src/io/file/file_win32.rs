use std::{
    io::{Result, SeekFrom},
    path::PathBuf,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll, Waker},
};

use futures::task::noop_waker_ref;
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
            _ = Pin::new(self).poll_close(&mut Context::from_waker(noop_waker_ref()));
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

    fn poll_close(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Result<()>> {
        unimplemented!()
    }

    fn poll_read<'cx>(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buffer: Self::ReadBuffer<'cx>,
        timeout: Option<std::time::Duration>,
    ) -> std::task::Poll<std::io::Result<usize>> {
        unimplemented!()
    }

    fn poll_write<'cx>(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buffer: Self::WriteBuffer<'cx>,

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
