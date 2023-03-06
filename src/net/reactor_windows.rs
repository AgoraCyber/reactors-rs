use std::{io::Result, time::Duration};

use futures::{future::BoxFuture, Future, FutureExt};

use crate::reactor::Reactor;

use super::{NetOpenOptions, ReadBuffer, WriteBuffer};

pub type SocketFd = windows::Win32::Networking::WinSock::SOCKET;

/// Reactor for windows network events
#[derive(Clone, Debug)]
pub struct NetReactor;

impl NetReactor {
    pub fn new() -> Self {
        NetReactor
    }

    pub fn poll_once(&mut self, _timeout: Duration) -> Result<()> {
        unimplemented!()
    }

    /// Get the account of io waiting tasks.
    pub fn wakers(&self) -> usize {
        0
    }
}

impl Reactor for NetReactor {
    type Handle = SocketFd;

    type Description = NetOpenOptions;

    type ReadBuffer<'cx> = ReadBuffer<'cx>;

    type WriteBuffer<'cx> = WriteBuffer<'cx>;

    type Open<'cx> = BoxFuture<'static, Result<SocketFd>>;

    type Write<'cx> = Write<'cx>;

    type Read<'cx> = Read<'cx>;

    fn close(&mut self, handle: Self::Handle) -> Result<()> {
        use windows::Win32::Networking::WinSock::*;

        // Ignore close error.
        unsafe { closesocket(handle) };

        Ok(())
    }

    fn open<'a, 'cx>(&'a mut self, description: Self::Description) -> Self::Open<'cx>
    where
        'a: 'cx,
    {
        description.into_fd(self.clone()).boxed()
    }

    fn read<'a, 'cx>(
        &'a mut self,
        handle: Self::Handle,
        buff: Self::ReadBuffer<'cx>,
    ) -> Self::Read<'cx>
    where
        'a: 'cx,
    {
        Read(handle, buff, self.clone())
    }

    fn write<'a, 'cx>(
        &'a mut self,
        handle: Self::Handle,
        buff: Self::WriteBuffer<'cx>,
    ) -> Self::Write<'cx>
    where
        'a: 'cx,
    {
        Write(handle, buff, self.clone())
    }
}

pub struct Write<'cx>(SocketFd, WriteBuffer<'cx>, NetReactor);

#[allow(unused)]
impl<'cx> Future for Write<'cx> {
    type Output = Result<usize>;
    fn poll(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        unimplemented!()
    }
}

pub struct Read<'cx>(SocketFd, ReadBuffer<'cx>, NetReactor);

#[allow(unused)]
impl<'cx> Future for Read<'cx> {
    type Output = Result<usize>;
    fn poll(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        unimplemented!()
    }
}
