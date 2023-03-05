use std::{
    io::{Error, Result},
    net::SocketAddr,
    os::fd::RawFd,
    task::Poll,
    time::Duration,
};

use errno::{errno, set_errno};
use futures::{future::BoxFuture, Future, FutureExt};
use os_socketaddr::OsSocketAddr;

use crate::reactor::{unix::UnixReactor, Reactor};

use super::NetOpenOptions;

use libc::*;

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
    Datagram(&'cx mut [u8], &'cx mut Option<SocketAddr>),
}

pub enum WriteBuffer<'cx> {
    Stream(&'cx [u8]),
    Datagram(&'cx [u8], SocketAddr),
}

#[allow(unused)]
impl Reactor for NetReactor {
    type Handle = RawFd;

    type Description = NetOpenOptions;

    type ReadBuffer<'cx> = ReadBuffer<'cx>;

    type WriteBuffer<'cx> = WriteBuffer<'cx>;

    type Open<'cx> = BoxFuture<'static, Result<RawFd>>;

    type Write<'cx> = Write<'cx>;

    type Read<'cx> = Read<'cx>;

    fn close(&mut self, handle: Self::Handle) -> Result<()> {
        // Ignore close error.
        unsafe { libc::close(handle) };

        Ok(())
    }

    fn open<'a, 'cx>(&'a mut self, description: Self::Description) -> Self::Open<'cx>
    where
        'a: 'cx,
    {
        description.into_fd().boxed()
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

pub struct Write<'cx>(RawFd, WriteBuffer<'cx>, NetReactor);

#[allow(unused)]
impl<'cx> Future for Write<'cx> {
    type Output = Result<usize>;
    fn poll(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        let len = match self.1 {
            WriteBuffer::Stream(buff) => unsafe {
                send(self.0, buff.as_ptr() as *const c_void, buff.len(), 0)
            },
            WriteBuffer::Datagram(buff, to) => unsafe {
                let addr: OsSocketAddr = to.into();

                sendto(
                    self.0,
                    buff.as_ptr() as *const c_void,
                    buff.len(),
                    0,
                    addr.as_ptr(),
                    addr.len(),
                )
            },
        };

        if len < 0 {
            let e = errno();

            set_errno(e);

            if e.0 == libc::EAGAIN || e.0 == libc::EWOULDBLOCK {
                let fd = self.0;
                // register event notify
                self.2 .0.event_writable_set(fd, cx.waker().clone());

                return Poll::Pending;
            } else {
                return Poll::Ready(Err(Error::from_raw_os_error(e.0)));
            }
        } else {
            log::trace!(target:"unix_net","fd({}) send bytes({})", self.0 , len);
            return Poll::Ready(Ok(len as usize));
        }
    }
}

pub struct Read<'cx>(RawFd, ReadBuffer<'cx>, NetReactor);

#[allow(unused)]
impl<'cx> Future for Read<'cx> {
    type Output = Result<usize>;
    fn poll(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        let fd = self.0;

        let len = match &mut self.as_mut().1 {
            ReadBuffer::Stream(buff) => unsafe {
                recv(fd, (*buff).as_mut_ptr() as *mut c_void, buff.len(), 0)
            },
            ReadBuffer::Datagram(buff, to) => unsafe {
                let mut addr = OsSocketAddr::new();
                let mut len = 0u32;
                recvfrom(
                    fd,
                    buff.as_ptr() as *mut c_void,
                    buff.len(),
                    0,
                    addr.as_mut_ptr(),
                    &mut len as *mut u32,
                )
            },
        };

        if len < 0 {
            let e = errno();

            set_errno(e);

            if e.0 == libc::EAGAIN || e.0 == libc::EWOULDBLOCK {
                let fd = self.0;
                // register event notify
                self.2 .0.event_readable_set(fd, cx.waker().clone());

                return Poll::Pending;
            } else {
                return Poll::Ready(Err(Error::from_raw_os_error(e.0)));
            }
        } else {
            log::trace!(target:"unix_net","fd({}) read bytes({})", self.0 , len);
            return Poll::Ready(Ok(len as usize));
        }
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
