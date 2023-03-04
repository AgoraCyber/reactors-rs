use std::{io::Result, net::SocketAddr, time::Duration};

use futures::Future;

use crate::reactor::{unix::UnixReactor, Reactor};

use super::SockOpenOps;

#[derive(Clone, Debug)]
pub struct SockHandle(i32);

/// Reactor for network events.
#[derive(Clone, Debug)]
pub struct NetReactor(UnixReactor);

impl NetReactor {
    /// Create new network reactor
    pub fn new() -> Self {
        Self(UnixReactor::new())
    }

    /// Run event dispatch once.
    pub fn poll_once(&self, timeout: Duration) -> Result<()> {
        self.0.poll_once(timeout)
    }
}

pub enum ReadBuffer<'cx> {
    Stream(&'cx mut [u8]),
    Datagram(&'cx mut [u8], Option<SocketAddr>),
}

pub enum WriteBuffer<'cx> {
    Stream(&'cx [u8]),
    Datagram(&'cx [u8], SocketAddr),
}

#[allow(unused)]
impl Reactor for NetReactor {
    type Handle = SockHandle;

    type Description = SockOpenOps;

    type ReadBuffer<'cx> = &'cx mut ReadBuffer<'cx>;

    type WriteBuffer<'cx> = &'cx WriteBuffer<'cx>;

    type Open<'cx> = Open;

    type Write<'cx> = Write<'cx>;

    type Read<'cx> = Read<'cx>;

    fn close(&mut self, handle: Self::Handle) -> Result<()> {
        unimplemented!()
    }

    fn open<'a, 'cx>(&'a mut self, description: Self::Description) -> Self::Open<'cx>
    where
        'a: 'cx,
    {
        unimplemented!()
    }

    fn read<'a, 'cx>(
        &'a mut self,
        handle: Self::Handle,
        buff: Self::ReadBuffer<'cx>,
    ) -> Self::Read<'cx>
    where
        'a: 'cx,
    {
        unimplemented!()
    }

    fn write<'a, 'cx>(
        &'a mut self,
        handle: Self::Handle,
        buff: Self::WriteBuffer<'cx>,
    ) -> Self::Write<'cx>
    where
        'a: 'cx,
    {
        unimplemented!()
    }
}

pub struct Open(Option<Result<SockHandle>>);

impl Future for Open {
    type Output = Result<SockHandle>;
    fn poll(
        mut self: std::pin::Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        std::task::Poll::Ready(self.0.take().unwrap())
    }
}

pub struct Write<'cx>(SockHandle, &'cx [u8], NetReactor);

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

pub struct Read<'cx>(SockHandle, &'cx mut [u8], NetReactor);

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

#[cfg(test)]
mod tests {
    #[test]
    fn test_enum_mut() {
        #[derive(Debug, PartialEq)]
        enum Hello {
            Message(Option<String>),
        }

        fn modify_enum(hello: &mut Hello) {
            match hello {
                Hello::Message(_) => *hello = Hello::Message(Some("hello".to_owned())),
            }
        }

        let mut hello = Hello::Message(None);

        modify_enum(&mut hello);

        assert_eq!(hello, Hello::Message(Some("hello".to_owned())));
    }
}
