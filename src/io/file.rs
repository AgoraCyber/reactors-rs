#[cfg_attr(target_family = "unix", path = "file_posix.rs")]
#[cfg_attr(target_family = "windows", path = "file_win32.rs")]
mod impls;
use std::{task::Poll, time::Duration};

use futures::{AsyncRead, AsyncSeek, AsyncWrite};
pub use impls::*;

use crate::{ReactorHandle, ReactorHandleSeekable};

use super::poller::SysPoller;

impl<P> FileHandle<P>
where
    P: SysPoller + Unpin + Clone + 'static,
{
    /// Convert file handle to [`AsyncRead`]
    pub fn to_read_stream(&self, timeout: Option<Duration>) -> FileReader<P> {
        FileReader(self.clone(), timeout)
    }

    /// Convert file handle to [`AsyncRead`]
    pub fn to_write_stream(&self, timeout: Option<Duration>) -> FileWriter<P> {
        FileWriter(self.clone(), timeout)
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
