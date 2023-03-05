//! cross-platform performance oriented file system async read/write api.

use futures::{AsyncRead, AsyncWrite, Future, FutureExt};
use std::{io::Result, path::PathBuf, task::Poll};

use crate::reactor::{Reactor, ReactorSeekable};

use super::{FileHandle, FileReactor, OpenOptions};

pub trait FileEx {
    type File: AsyncWrite + AsyncRead + Unpin + 'static;

    type Open<'cx>: Future<Output = Result<Self::File>> + 'cx
    where
        Self: 'cx;
    fn create_file<'a, 'cx, P: Into<PathBuf>>(&'a mut self, path: P) -> Self::Open<'cx>
    where
        'a: 'cx,
    {
        self.open_file(
            OpenOptions::build()
                .read(true)
                .truncate(true)
                .path(path.into()),
        )
    }

    fn read_file<'a, 'cx, P: Into<PathBuf>>(&'a mut self, path: P) -> Self::Open<'cx>
    where
        'a: 'cx,
    {
        self.open_file(OpenOptions::build().read(true).path(path.into()))
    }

    fn open_file<'a, 'cx>(&'a mut self, ops: OpenOptions) -> Self::Open<'cx>
    where
        'a: 'cx;
}

impl FileEx for FileReactor {
    type File = File;

    type Open<'cx> = FileOpen;
    fn open_file<'a, 'cx>(&'a mut self, ops: OpenOptions) -> Self::Open<'cx>
    where
        'a: 'cx,
    {
        FileOpen(self.clone(), ops)
    }
}

pub struct FileOpen(FileReactor, OpenOptions);

impl Future for FileOpen {
    type Output = Result<File>;
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

/// [`futures::AsyncWrite`] + [`futures::AsyncRead`] implementation.
#[derive(Clone, Debug)]
pub struct File {
    pub reactor: FileReactor,
    pub handle: FileHandle,
}

impl From<(FileReactor, FileHandle)> for File {
    fn from(value: (FileReactor, FileHandle)) -> Self {
        Self {
            reactor: value.0,
            handle: value.1,
        }
    }
}

impl Drop for File {
    fn drop(&mut self) {
        let handle = self.handle.clone();
        let mut reactor = self.reactor.clone();

        reactor.close(handle).unwrap();
    }
}

impl futures::AsyncWrite for File {
    fn poll_write(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<Result<usize>> {
        let handle = self.handle.clone();
        let reactor = &mut self.reactor;

        let mut write = reactor.write(handle, buf);

        write.poll_unpin(cx)
    }

    fn poll_close(
        self: std::pin::Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<()>> {
        let handle = self.handle.clone();
        let mut reactor = self.reactor.clone();

        Poll::Ready(reactor.close(handle))
    }

    fn poll_flush(
        self: std::pin::Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<()>> {
        Poll::Ready(Ok(()))
    }
}

impl futures::AsyncRead for File {
    fn poll_read(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut [u8],
    ) -> std::task::Poll<Result<usize>> {
        let handle = self.handle.clone();
        let mut reactor = self.reactor.clone();

        let mut read = reactor.read(handle, buf);

        read.poll_unpin(cx)
    }
}

impl futures::AsyncSeek for File {
    fn poll_seek(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        pos: std::io::SeekFrom,
    ) -> Poll<Result<u64>> {
        let handle = self.handle.clone();
        let mut reactor = self.reactor.clone();

        let mut seek = reactor.seek(handle, pos);

        seek.poll_unpin(cx)
    }
}
