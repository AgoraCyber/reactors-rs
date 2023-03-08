use std::{
    io::{Error, Result},
    mem::size_of,
    net::SocketAddr,
    sync::Arc,
    task::{Poll, Waker},
};

use futures::task::noop_waker;
use os_socketaddr::OsSocketAddr;

use crate::{
    io::poller::{PollerReactor, SysPoller},
    ReactorHandle,
};

use super::{SocketReadBuffer, SocketWriteBuffer};

use errno::*;

#[derive(Clone, Debug)]
pub struct SocketHandle<P>
where
    P: SysPoller + Unpin + Clone + 'static,
{
    poller: PollerReactor<P>,
    fd: Arc<i32>,
}

impl<P> Drop for SocketHandle<P>
where
    P: SysPoller + Unpin + Clone + 'static,
{
    fn drop(&mut self) {
        // Only self
        if Arc::strong_count(&self.fd) == 1 {
            _ = self.poll_close(noop_waker());
        }
    }
}

impl<P> SocketHandle<P>
where
    P: SysPoller + Unpin + Clone + 'static,
{
    /// Create socket handle from raw socket fd.
    pub fn new(poller: PollerReactor<P>, fd: i32) -> Self {
        Self {
            poller,
            fd: Arc::new(fd),
        }
    }
    /// Create udp socket with [`addr`](SocketAddr)
    pub fn udp(poller: PollerReactor<P>, addr: SocketAddr) -> Result<Self> {
        use libc::*;

        unsafe {
            let fd = match addr {
                SocketAddr::V4(_) => socket(AF_INET, SOCK_DGRAM, 0),
                SocketAddr::V6(_) => socket(AF_INET6, SOCK_DGRAM, 0),
            };

            if fd < 0 {
                return Err(Error::last_os_error());
            }

            crate::io::noblock(fd)?;

            let addr: OsSocketAddr = addr.into();

            if bind(fd, addr.as_ptr(), addr.len()) < 0 {
                return Err(Error::last_os_error());
            }

            Ok(Self::new(poller, fd))
        }
    }

    /// Create tcp socket
    pub fn tcp(poller: PollerReactor<P>, bind_addr: SocketAddr) -> Result<Self> {
        use libc::*;

        unsafe {
            let fd = match bind_addr {
                SocketAddr::V4(_) => socket(AF_INET, SOCK_STREAM, 0),
                SocketAddr::V6(_) => socket(AF_INET6, SOCK_STREAM, 0),
            };

            if fd < 0 {
                return Err(Error::last_os_error());
            }

            crate::io::noblock(fd)?;

            let addr: OsSocketAddr = bind_addr.into();

            if bind(fd, addr.as_ptr(), addr.len()) < 0 {
                return Err(Error::last_os_error());
            }

            Ok(Self::new(poller, fd))
        }
    }

    /// Tcep acceptor socket start listening incoming connection
    pub fn listen(&mut self) -> Result<()> {
        use libc::*;

        unsafe {
            let on: c_int = 1;

            let len = size_of::<c_int>() as u32;

            if setsockopt(
                *self.fd,
                SOL_SOCKET,
                SO_REUSEADDR,
                on.to_be_bytes().as_ptr() as *const libc::c_void,
                len,
            ) < 0
            {
                return Err(Error::last_os_error());
            }

            if listen(*self.fd, SOMAXCONN) < 0 {
                return Err(Error::last_os_error());
            } else {
                Ok(())
            }
        }
    }
    pub fn poll_connect(
        &mut self,
        remote: SocketAddr,
        waker: std::task::Waker,
        timeout: Option<std::time::Duration>,
    ) -> std::task::Poll<std::io::Result<()>> {
        use libc::*;

        let remote: OsSocketAddr = remote.into();

        let len = unsafe {
            let err_no: c_int = 0;

            let mut len = size_of::<c_int>() as u32;

            if libc::getsockopt(
                *self.fd,
                libc::SOL_SOCKET,
                libc::SO_ERROR,
                err_no.to_be_bytes().as_mut_ptr() as *mut libc::c_void,
                &mut len as *mut u32,
            ) < 0
            {
                return Poll::Ready(Err(Error::last_os_error()));
            }

            if err_no != 0 {
                return Poll::Ready(Err(Error::from_raw_os_error(err_no)));
            }

            connect(*self.fd, remote.as_ptr(), remote.len())
        };

        if len < 0 {
            let e = errno();

            set_errno(e);

            if e.0 == libc::EAGAIN || e.0 == libc::EWOULDBLOCK || e.0 == libc::EINPROGRESS {
                return match self
                    .poller
                    .watch_writable_event_once(*self.fd, waker, timeout)
                {
                    Ok(_) => Poll::Pending,
                    Err(err) => Poll::Ready(Err(err)),
                };
            } else if EISCONN == e.0 {
                return Poll::Ready(Ok(()));
            } else {
                return Poll::Ready(Err(Error::from_raw_os_error(e.0)));
            }
        } else {
            return Poll::Ready(Ok(()));
        }
    }
}

impl<P> ReactorHandle for SocketHandle<P>
where
    P: SysPoller + Unpin + Clone + 'static,
{
    type ReadBuffer<'cx> = SocketReadBuffer<'cx, P>;
    type WriteBuffer<'cx> = SocketWriteBuffer<'cx>;

    fn poll_close(&mut self, _waker: Waker) -> Poll<Result<()>> {
        use libc::*;

        log::trace!("close socket({})", *self.fd);

        if unsafe { close(*self.fd) } < 0 {
            Poll::Ready(Err(Error::last_os_error()))
        } else {
            Poll::Ready(Ok(()))
        }
    }

    fn poll_read<'cx>(
        &mut self,
        buffer: Self::ReadBuffer<'cx>,
        waker: std::task::Waker,
        timeout: Option<std::time::Duration>,
    ) -> std::task::Poll<std::io::Result<usize>> {
        use libc::*;

        let len = match buffer {
            SocketReadBuffer::Accept(handle, peer) => unsafe {
                let mut remote = [0u8; size_of::<sockaddr_in6>()];

                let mut len = remote.len() as u32;

                let conn_fd = accept(
                    *self.fd,
                    remote.as_mut_ptr() as *mut sockaddr,
                    &mut len as *mut u32,
                );

                if conn_fd != -1 {
                    let addr = OsSocketAddr::copy_from_raw(
                        remote.as_mut_ptr() as *mut sockaddr,
                        len as socklen_t,
                    );

                    *peer = addr.into_addr();

                    *handle = Some(SocketHandle::new(self.poller.clone(), conn_fd));

                    log::trace!(target:"unix_net","fd({}) accept connection({}) from ({:?})", self.fd, conn_fd, peer);

                    0
                } else {
                    -1
                }
            },
            SocketReadBuffer::Datagram(buf, remote) => unsafe {
                let mut remote_buff = [0u8; size_of::<sockaddr_in6>()];

                let mut len = remote_buff.len() as u32;

                let len = recvfrom(
                    *self.fd,
                    buf.as_ptr() as *mut c_void,
                    buf.len(),
                    0,
                    remote_buff.as_mut_ptr() as *mut sockaddr,
                    &mut len as *mut u32,
                );

                if len >= 0 {
                    let addr = OsSocketAddr::copy_from_raw(
                        remote_buff.as_mut_ptr() as *mut sockaddr,
                        len as socklen_t,
                    );

                    *remote = addr.into_addr();

                    log::trace!(target:"unix_net","fd({}) recvfrom({:?}) {}", self.fd, remote, len);
                }

                len
            },
            SocketReadBuffer::Stream(buf) => unsafe {
                recv(*self.fd, (*buf).as_mut_ptr() as *mut c_void, buf.len(), 0)
            },
        };

        log::trace!("socket({}) read bytes({})", *self.fd, len);

        if len < 0 {
            let e = errno();

            set_errno(e);

            if e.0 == libc::EAGAIN || e.0 == libc::EWOULDBLOCK {
                return match self
                    .poller
                    .watch_readable_event_once(*self.fd, waker, timeout)
                {
                    Ok(_) => Poll::Pending,
                    Err(err) => Poll::Ready(Err(err)),
                };
            } else {
                return Poll::Ready(Err(Error::from_raw_os_error(e.0)));
            }
        } else {
            return Poll::Ready(Ok(len as usize));
        }
    }

    fn poll_write<'cx>(
        &mut self,
        buffer: Self::WriteBuffer<'cx>,
        waker: std::task::Waker,
        timeout: Option<std::time::Duration>,
    ) -> std::task::Poll<std::io::Result<usize>> {
        use libc::*;

        let len = match buffer {
            SocketWriteBuffer::Datagram(buf, to) => unsafe {
                let addr: OsSocketAddr = to.clone().into();

                let len = sendto(
                    *self.fd,
                    buf.as_ptr() as *const c_void,
                    buf.len(),
                    0,
                    addr.as_ptr(),
                    addr.len(),
                );

                log::trace!(target:"unix_net","fd({}) sendto({}) {:?}", self.fd, to, len);

                len
            },
            SocketWriteBuffer::Stream(buf) => unsafe {
                send(*self.fd, buf.as_ptr() as *const c_void, buf.len(), 0)
            },
        };

        log::trace!("socket({}) write bytes({})", *self.fd, len);

        if len < 0 {
            let e = errno();

            set_errno(e);

            if e.0 == libc::EAGAIN || e.0 == libc::EWOULDBLOCK {
                return match self
                    .poller
                    .watch_writable_event_once(*self.fd, waker, timeout)
                {
                    Ok(_) => Poll::Pending,
                    Err(err) => Poll::Ready(Err(err)),
                };
            } else {
                return Poll::Ready(Err(Error::from_raw_os_error(e.0)));
            }
        } else {
            return Poll::Ready(Ok(len as usize));
        }
    }
}
