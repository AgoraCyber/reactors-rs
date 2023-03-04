use std::{
    future::Future,
    io::{Result, SeekFrom},
    task::Poll,
};

use futures::FutureExt;

/// Cross-platform event loop reactor.
pub trait Reactor {
    /// File handle
    type Handle: Unpin;
    /// File description and open parameters.
    type Description: Unpin;

    /// Buffer type for file read/write operators.
    type WriteBuffer<'cx>: Unpin
    where
        Self: 'cx;

    type ReadBuffer<'cx>
    where
        Self: 'cx;

    /// returns by [`open`](Reactor::open) method
    type Open<'cx>: Future<Output = Result<Self::Handle>> + Unpin + 'cx
    where
        Self: 'cx;

    /// returns by [`close`](Reactor::close) method
    type Close<'cx>: Future<Output = Result<()>> + Unpin + 'cx
    where
        Self: 'cx;

    /// returns by [`write`](Reactor::write) method
    type Write<'cx>: Future<Output = Result<usize>> + Unpin + 'cx
    where
        Self: 'cx;

    /// returns by [`read`](Reactor::read) method
    type Read<'cx>: Future<Output = Result<usize>> + Unpin + 'cx
    where
        Self: 'cx;

    /// Open file with description and returns handle.
    fn open<'a, 'cx>(&'a mut self, description: Self::Description) -> Self::Open<'cx>
    where
        'a: 'cx;

    /// Close file by [`handle`](FileHandle).
    fn close<'a, 'cx>(&'a mut self, handle: Self::Handle) -> Self::Close<'cx>
    where
        'a: 'cx;

    /// Write data to file
    fn write<'a, 'cx>(
        &'a mut self,
        handle: Self::Handle,
        buff: Self::WriteBuffer<'cx>,
    ) -> Self::Write<'cx>
    where
        'a: 'cx;

    /// Read data from file
    fn read<'a, 'cx>(
        &'a mut self,
        handle: Self::Handle,
        buff: Self::ReadBuffer<'cx>,
    ) -> Self::Read<'cx>
    where
        'a: 'cx;
}

pub trait ReactorSeekable {
    /// File handle
    type Handle;

    /// returns by [`seek`](Reactor::seek) method
    type Seek<'cx>: Future<Output = Result<usize>> + 'cx
    where
        Self: 'cx;

    /// Try to seek in file stream.
    ///
    /// # implementation
    ///
    /// If the file type doesn't support seek, should returns error.
    fn seek<'a, 'cx>(&'a mut self, handle: Self::Handle, pos: SeekFrom) -> Self::Seek<'cx>
    where
        'a: 'cx;
}

/// [`futures::AsyncRead`] implementation.
pub struct AsyncRead<R>
where
    for<'a> R: Reactor<ReadBuffer<'a> = &'a mut [u8]> + Unpin + Clone + 'static,
    R::Handle: Clone,
{
    pub reactor: R,
    pub handle: R::Handle,
}

impl<R> From<(R, R::Handle)> for AsyncRead<R>
where
    for<'a> R: Reactor<ReadBuffer<'a> = &'a mut [u8]> + Unpin + Clone + 'static,
    R::Handle: Clone,
{
    fn from(value: (R, R::Handle)) -> Self {
        Self {
            reactor: value.0,
            handle: value.1,
        }
    }
}

impl<R> futures::AsyncRead for AsyncRead<R>
where
    for<'a> R: Reactor<ReadBuffer<'a> = &'a mut [u8]> + Unpin + Clone + 'static,
    R::Handle: Clone,
{
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

/// [`futures::AsyncWrite`] implementation.
pub struct AsyncWrite<R>
where
    for<'a> R: Reactor<WriteBuffer<'a> = &'a [u8]> + Unpin + Clone + 'static,
    R::Handle: Clone,
{
    pub reactor: R,
    pub handle: R::Handle,
}

impl<R> From<(R, R::Handle)> for AsyncWrite<R>
where
    for<'a> R: Reactor<WriteBuffer<'a> = &'a [u8]> + Unpin + Clone + 'static,
    R::Handle: Clone,
{
    fn from(value: (R, R::Handle)) -> Self {
        Self {
            reactor: value.0,
            handle: value.1,
        }
    }
}

impl<R> futures::AsyncWrite for AsyncWrite<R>
where
    for<'a> R: Reactor<WriteBuffer<'a> = &'a [u8]> + Unpin + Clone + 'static,
    R::Handle: Clone,
{
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
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<()>> {
        let handle = self.handle.clone();
        let mut reactor = self.reactor.clone();

        let mut close = reactor.close(handle);

        close.poll_unpin(cx)
    }

    fn poll_flush(
        self: std::pin::Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<()>> {
        Poll::Ready(Ok(()))
    }
}
