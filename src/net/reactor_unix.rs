use std::{
    io::{Error, Result},
    mem::size_of,
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
pub struct NetReactor(pub(crate) UnixReactor);

impl NetReactor {
    /// Create new network reactor
    pub fn new() -> Self {
        Self(UnixReactor::new())
    }

    /// Run event dispatch once.
    pub fn poll_once(&self, timeout: Duration) -> Result<()> {
        self.0.poll_once(timeout)
    }

    /// Get the account of io waiting tasks.
    pub fn wakers(&self) -> usize {
        self.0.wakers()
    }
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

                let len = sendto(
                    self.0,
                    buff.as_ptr() as *const c_void,
                    buff.len(),
                    0,
                    addr.as_ptr(),
                    addr.len(),
                );

                log::trace!(target:"unix_net","fd({}) sendto({}) {:?}", self.0, to, len);

                len
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
        log::trace!(target:"unix_net","fd({}) read bytes", self.0 );

        let fd = self.0;

        let len = match &mut self.as_mut().1 {
            ReadBuffer::Accept(conn_fd, peer) => unsafe {
                let mut remote = [0u8; size_of::<sockaddr_in6>()];

                let mut len = remote.len() as u32;

                let len = accept(
                    fd,
                    remote.as_mut_ptr() as *mut sockaddr,
                    &mut len as *mut u32,
                );

                if len != -1 {
                    let addr = OsSocketAddr::copy_from_raw(
                        remote.as_mut_ptr() as *mut sockaddr,
                        len as socklen_t,
                    );

                    **peer = addr.into_addr();

                    **conn_fd = Some(len);

                    log::trace!(target:"unix_net","fd({}) accept connection({}) from ({:?})", fd, len, peer);

                    0
                } else {
                    -1
                }
            },
            ReadBuffer::Stream(buff) => unsafe {
                recv(fd, (*buff).as_mut_ptr() as *mut c_void, buff.len(), 0)
            },
            ReadBuffer::Datagram(buff, to) => unsafe {
                let mut remote = [0u8; size_of::<sockaddr_in6>()];

                let mut len = remote.len() as u32;

                let len = recvfrom(
                    fd,
                    buff.as_ptr() as *mut c_void,
                    buff.len(),
                    0,
                    remote.as_mut_ptr() as *mut sockaddr,
                    &mut len as *mut u32,
                );

                if len >= 0 {
                    let addr = OsSocketAddr::copy_from_raw(
                        remote.as_mut_ptr() as *mut sockaddr,
                        len as socklen_t,
                    );

                    **to = addr.into_addr();

                    log::trace!(target:"unix_net","fd({}) recvfrom({:?}) {}", fd, to, len);
                }

                len
            },
        };

        log::trace!(target:"unix_net","fd({}) read bytes({})", self.0 , len);

        if len < 0 {
            let e = errno();

            set_errno(e);

            if e.0 == libc::EAGAIN || e.0 == libc::EWOULDBLOCK {
                let fd = self.0;
                // register event notify
                self.2 .0.event_readable_set(fd, cx.waker().clone());

                log::trace!(target:"unix_net","fd({}) WOULDBLOCK", fd);

                return Poll::Pending;
            } else {
                return Poll::Ready(Err(Error::from_raw_os_error(e.0)));
            }
        } else {
            return Poll::Ready(Ok(len as usize));
        }
    }
}

impl NetOpenOptions {
    fn udp(addr: SocketAddr) -> Result<SocketFd> {
        Self::sock(&addr, libc::SOCK_DGRAM, true)
    }

    fn tcp_listener(addr: SocketAddr) -> Result<SocketFd> {
        use libc::*;

        let fd = Self::sock(&addr, libc::SOCK_STREAM, true)?;

        unsafe {
            if listen(fd, SOMAXCONN) < 0 {
                return Err(Error::last_os_error());
            }

            Ok(fd)
        }
    }

    fn tcp_connect(
        reactor: NetReactor,
        to: SocketAddr,
        bind_addr: Option<SocketAddr>,
    ) -> Result<OpenTcpConnect> {
        let fd = if let Some(addr) = bind_addr {
            Self::sock(&addr, libc::SOCK_STREAM, true)?
        } else {
            Self::sock(&to, libc::SOCK_STREAM, false)?
        };

        let addr: OsSocketAddr = to.clone().into();

        Ok(OpenTcpConnect(reactor, Some(fd), addr))
    }

    fn sock(addr: &SocketAddr, ty: c_int, bind_addr: bool) -> Result<SocketFd> {
        use libc::*;

        unsafe {
            let fd = match addr {
                SocketAddr::V4(_) => socket(AF_INET, ty, 0),
                SocketAddr::V6(_) => socket(AF_INET6, ty, 0),
            };

            // Set O_NONBLOCK

            let flags = fcntl(fd, F_GETFL);

            if flags < 0 {
                return Err(Error::last_os_error());
            }

            if fcntl(fd, F_SETFL, flags | O_NONBLOCK) < 0 {
                return Err(Error::last_os_error());
            }

            // Set SO_REUSEADDR
            if bind_addr {
                if ty == SOCK_STREAM {
                    let one: c_int = 1;

                    if setsockopt(
                        fd,
                        SOL_SOCKET,
                        SO_REUSEADDR,
                        one.to_be_bytes().as_ptr() as *const c_void,
                        size_of::<c_int>() as u32,
                    ) < 0
                    {
                        return Err(Error::last_os_error());
                    }
                }

                let addr: OsSocketAddr = addr.clone().into();

                if bind(fd, addr.as_ptr(), addr.len()) < 0 {
                    return Err(Error::last_os_error());
                }
            }

            Ok(fd)
        }
    }
}
