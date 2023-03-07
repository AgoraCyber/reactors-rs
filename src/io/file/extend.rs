use std::{io::Result, task::Poll, time::Duration};

use futures::{task::noop_waker, AsyncRead, AsyncSeek, AsyncWrite};

use crate::{
    io::poller::{PollerWrapper, SysPoller},
    ReactorHandle, ReactorHandleSeekable,
};

use super::FileHandle;

/// Tcp connection socket facade.
pub struct File<P>(FileHandle<P>)
where
    P: SysPoller + Unpin + Clone + 'static;

impl<P> File<P>
where
    P: SysPoller + Unpin + Clone + 'static,
{
    pub fn create(poller: PollerWrapper<P>, path: &str) -> Result<Self> {
        FileHandle::create(poller, path).map(|h| Self(h))
    }

    pub fn open(poller: PollerWrapper<P>, path: &str) -> Result<Self> {
        FileHandle::open(poller, path).map(|h| Self(h))
    }

    /// Convert file handle to [`AsyncRead`]
    pub fn to_read_stream(&self, timeout: Option<Duration>) -> FileReader<P> {
        FileReader(self.0.clone(), timeout)
    }

    /// Convert file handle to [`AsyncRead`]
    pub fn to_write_stream(&self, timeout: Option<Duration>) -> FileWriter<P> {
        FileWriter(self.0.clone(), timeout)
    }
}

impl<P> Drop for File<P>
where
    P: SysPoller + Unpin + Clone + 'static,
{
    fn drop(&mut self) {
        _ = self.0.poll_close(noop_waker());
    }
}

pub struct FileReader<P>(FileHandle<P>, Option<Duration>)
where
    P: SysPoller + Unpin + Clone + 'static;

impl<P> AsyncRead for FileReader<P>
where
    P: SysPoller + Unpin + Clone + 'static,
{
    fn poll_read(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut [u8],
    ) -> std::task::Poll<std::io::Result<usize>> {
        let timeout = self.1.clone();

        self.0.poll_read(buf, cx.waker().clone(), timeout)
    }
}

impl<P> AsyncSeek for FileReader<P>
where
    P: SysPoller + Unpin + Clone + 'static,
{
    fn poll_seek(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        pos: std::io::SeekFrom,
    ) -> Poll<std::io::Result<u64>> {
        let timeout = self.1.clone();
        self.0.seek(pos, cx.waker().clone(), timeout)
    }
}

pub struct FileWriter<P>(FileHandle<P>, Option<Duration>)
where
    P: SysPoller + Unpin + Clone + 'static;

impl<P> AsyncWrite for FileReader<P>
where
    P: SysPoller + Unpin + Clone + 'static,
{
    fn poll_close(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        self.0.poll_close(cx.waker().clone())
    }

    fn poll_flush(
        self: std::pin::Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        Poll::Ready(Ok(()))
    }

    fn poll_write(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<std::io::Result<usize>> {
        let timeout = self.1.clone();

        self.0.poll_write(buf, cx.waker().clone(), timeout)
    }
}

impl<P> AsyncSeek for FileWriter<P>
where
    P: SysPoller + Unpin + Clone + 'static,
{
    fn poll_seek(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        pos: std::io::SeekFrom,
    ) -> Poll<std::io::Result<u64>> {
        let timeout = self.1.clone();
        self.0.seek(pos, cx.waker().clone(), timeout)
    }
}
