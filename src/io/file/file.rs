//! File with asynchronous io support

use std::{fs::OpenOptions, io::Result, pin::Pin, task::Poll, time::Duration};

use futures::{AsyncRead, AsyncSeek, AsyncWrite};
use std::path::PathBuf;

use crate::{io::IoReactor, ReactorHandle, ReactorHandleSeekable};

use super::Handle;

/// Tcp connection socket facade.
pub struct File(Handle);

impl File {
    /// Create new file with asynchronous read/write suppport, if the file exists, truncate it.
    pub fn create<PB: Into<PathBuf>>(poller: IoReactor, path: PB) -> Result<Self> {
        use super::sys::File;

        Handle::new(
            poller,
            path.into(),
            OpenOptions::new()
                .create(true)
                .truncate(true)
                .write(true)
                .read(true),
        )
        .map(|h| Self(h))
    }

    /// Open exists file with asynchronous read/write suppport.
    pub fn open<PB: Into<PathBuf>>(poller: IoReactor, path: PB) -> Result<Self> {
        use super::sys::File;

        Handle::new(
            poller,
            path.into(),
            OpenOptions::new().read(true).write(true),
        )
        .map(|h| Self(h))
    }

    /// Convert file handle to [`AsyncRead`]
    pub fn to_read_stream<T: Into<Option<Duration>>>(&self, timeout: T) -> FileReader {
        FileReader(self.0.clone(), timeout.into())
    }

    /// Convert file handle to [`AsyncRead`]
    pub fn to_write_stream<T: Into<Option<Duration>>>(&self, timeout: T) -> FileWriter {
        FileWriter(self.0.clone(), timeout.into())
    }
}

/// File reader stream with operator timeout support
pub struct FileReader(Handle, Option<Duration>);

impl AsyncRead for FileReader {
    fn poll_read(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut [u8],
    ) -> std::task::Poll<std::io::Result<usize>> {
        let timeout = self.1.clone();

        Pin::new(&mut self.0).poll_read(cx, buf, timeout)
    }
}

impl AsyncSeek for FileReader {
    fn poll_seek(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        pos: std::io::SeekFrom,
    ) -> Poll<std::io::Result<u64>> {
        let timeout = self.1.clone();
        self.0.seek(pos, cx.waker().clone(), timeout)
    }
}

/// File writer stream with operator timeout support.
pub struct FileWriter(Handle, Option<Duration>);

impl AsyncWrite for FileWriter {
    fn poll_close(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        Pin::new(&mut self.0).poll_close(cx)
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

        Pin::new(&mut self.0).poll_write(cx, buf, timeout)
    }
}

impl AsyncSeek for FileWriter {
    fn poll_seek(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        pos: std::io::SeekFrom,
    ) -> Poll<std::io::Result<u64>> {
        let timeout = self.1.clone();
        self.0.seek(pos, cx.waker().clone(), timeout)
    }
}
