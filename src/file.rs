//! cross-platform performance oriented file system async read/write api.

// #[cfg_attr(target_os = "macos", path = "file/kqueue.rs")]
// #[cfg_attr(target_os = "ios", path = "file/kqueue.rs")]
// #[cfg_attr(target_os = "freebsd", path = "file/kqueue.rs")]
// mod impls;

#[cfg_attr(target_family = "unix", path = "file_unix.rs")]
mod impls;
use futures::{AsyncWrite, Future, FutureExt};
pub use impls::*;

use std::{io::Result, path::PathBuf, task::Poll};

use crate::reactor::Reactor;

/// File open description
#[derive(Debug, Clone)]
pub struct OpenOptions {
    pub read: bool,
    pub write: bool,
    pub append_or_truncate: bool,
    pub path: PathBuf,
}

#[derive(Default, Debug)]
pub struct OpenOptionsBuilder {
    pub read: bool,
    pub write: bool,
    pub append_or_truncate: bool,
}

impl OpenOptions {
    /// Create new open options
    pub fn build() -> OpenOptionsBuilder {
        OpenOptionsBuilder::default()
    }
}
impl OpenOptionsBuilder {
    pub fn read(mut self, flag: bool) -> Self {
        self.read = flag;

        self
    }

    pub fn write(mut self, flag: bool) -> Self {
        self.write = flag;

        self
    }

    pub fn append(mut self, flag: bool) -> Self {
        self.append_or_truncate = flag;

        self
    }

    pub fn truncate(mut self, flag: bool) -> Self {
        self.append_or_truncate = !flag;

        self
    }

    pub fn path<P: Into<PathBuf>>(self, path: P) -> OpenOptions {
        OpenOptions {
            read: self.read,
            write: self.write,
            append_or_truncate: self.append_or_truncate,
            path: path.into(),
        }
    }
}

pub trait File {
    type Write: AsyncWrite + Unpin + 'static;

    type Create<'cx>: Future<Output = Result<Self::Write>> + 'cx
    where
        Self: 'cx;
    fn create_file<'a, 'cx, P: Into<PathBuf>>(&mut self, path: P) -> Self::Create<'cx>
    where
        'a: 'cx;
}

impl File for FileReactor {
    type Write = crate::reactor::AsyncWrite<FileReactor>;

    type Create<'cx> = FileCreate;
    fn create_file<'a, 'cx, P: Into<PathBuf>>(&mut self, path: P) -> Self::Create<'cx>
    where
        'a: 'cx,
    {
        let ops = OpenOptions::build().path(path);

        FileCreate(self.clone(), ops)
    }
}

pub struct FileCreate(FileReactor, OpenOptions);

impl Future for FileCreate {
    type Output = Result<crate::reactor::AsyncWrite<FileReactor>>;
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
