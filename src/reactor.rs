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
    fn close(&mut self, handle: Self::Handle) -> Result<()>;

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
    type Seek<'cx>: Future<Output = Result<u64>> + 'cx
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

/// [`futures::AsyncWrite`] + [`futures::AsyncRead`] implementation.
pub struct AsyncReadWrite<R>
where
    for<'a> R: Reactor<WriteBuffer<'a> = &'a [u8], ReadBuffer<'a> = &'a mut [u8]>
        + Unpin
        + Clone
        + 'static,
    R::Handle: Clone,
{
    pub reactor: R,
    pub handle: R::Handle,
}

impl<R> From<(R, R::Handle)> for AsyncReadWrite<R>
where
    for<'a> R: Reactor<WriteBuffer<'a> = &'a [u8], ReadBuffer<'a> = &'a mut [u8]>
        + Unpin
        + Clone
        + 'static,
    R::Handle: Clone,
{
    fn from(value: (R, R::Handle)) -> Self {
        Self {
            reactor: value.0,
            handle: value.1,
        }
    }
}

impl<R> futures::AsyncWrite for AsyncReadWrite<R>
where
    for<'a> R: Reactor<WriteBuffer<'a> = &'a [u8], ReadBuffer<'a> = &'a mut [u8]>
        + Unpin
        + Clone
        + 'static,
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

impl<R> futures::AsyncRead for AsyncReadWrite<R>
where
    for<'a> R: Reactor<WriteBuffer<'a> = &'a [u8], ReadBuffer<'a> = &'a mut [u8]>
        + Unpin
        + Clone
        + 'static,
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

#[cfg(target_family = "unix")]
pub mod unix {

    use crate::poller::*;

    use std::{
        collections::HashMap,
        io::Result,
        sync::{Arc, Mutex},
        task::Waker,
        time::Duration,
    };

    #[derive(Debug, Default)]
    pub(crate) struct Wakers {
        read_ops: HashMap<i32, Waker>,
        write_ops: HashMap<i32, Waker>,
    }

    impl Wakers {
        fn append_read(&mut self, fd: i32, waker: Waker) {
            self.read_ops.insert(fd, waker);
        }

        fn append_write(&mut self, fd: i32, waker: Waker) {
            self.write_ops.insert(fd, waker);
        }

        fn to_poll_events(&self) -> Vec<PollEvent> {
            let mut poll_events = vec![];

            for (fd, _) in &self.read_ops {
                poll_events.push(PollEvent::Readable(*fd));
            }

            for (fd, _) in &self.write_ops {
                poll_events.push(PollEvent::Writable(*fd));
            }

            poll_events
        }

        fn remove_fired_wakers(&mut self, events: &[PollEvent]) -> Vec<Waker> {
            let mut wakers = vec![];

            for event in events {
                match event {
                    PollEvent::Readable(fd) => {
                        if let Some(waker) = self.read_ops.remove(fd) {
                            wakers.push(waker);
                        }
                    }
                    PollEvent::Writable(fd) => {
                        if let Some(waker) = self.write_ops.remove(fd) {
                            wakers.push(waker);
                        }
                    }
                }
            }

            return wakers;
        }
    }

    /// Reactor for unix family
    #[derive(Clone, Debug)]
    pub struct UnixReactor {
        poller: UnixPoller,
        pub(crate) wakers: Arc<Mutex<Wakers>>,
    }

    impl UnixReactor {
        pub fn new() -> Self {
            Self {
                poller: UnixPoller::new(),
                wakers: Default::default(),
            }
        }
        /// Invoke poll procedure once,
        pub fn poll_once(&self, timeout: Duration) -> Result<()> {
            let events = self.wakers.lock().unwrap().to_poll_events();

            let fired = self.poller.poll_once(&events, timeout)?;

            let wakers = self.wakers.lock().unwrap().remove_fired_wakers(&fired);

            for waker in wakers {
                waker.wake_by_ref();
            }

            Ok(())
        }

        pub(crate) fn event_readable_set(&mut self, fd: i32, waker: Waker) {
            self.wakers.lock().unwrap().append_read(fd, waker);
        }

        pub(crate) fn event_writable_set(&mut self, fd: i32, waker: Waker) {
            self.wakers.lock().unwrap().append_write(fd, waker);
        }
    }
}
