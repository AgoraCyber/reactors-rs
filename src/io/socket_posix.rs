use std::{
    io::{Error, ErrorKind, Result},
    mem::size_of,
    task::{Poll, Waker},
    time::SystemTime,
};

use os_socketaddr::OsSocketAddr;

use crate::{
    io::poller::{PollerWrapper, SysPoller},
    ReactorHandle,
};

use super::{SocketReadBuffer, SocketWriteBuffer};

use errno::*;

#[derive(Clone, Debug)]
pub struct SocketHandle<P>
where
    P: SysPoller + Unpin + Clone + 'static,
{
    poller: PollerWrapper<P>,
    fd: i32,
    last_read_poll_time: Option<SystemTime>,
    last_write_poll_time: Option<SystemTime>,
}

impl<P> SocketHandle<P>
where
    P: SysPoller + Unpin + Clone + 'static,
{
    /// Create socket handle from raw socket fd.
    pub fn new(poller: PollerWrapper<P>, fd: i32) -> Self {
        Self {
            poller,
            fd,
            last_read_poll_time: Default::default(),
            last_write_poll_time: Default::default(),
        }
    }
}

#[allow(unused)]
impl<P> ReactorHandle for SocketHandle<P>
where
    P: SysPoller + Unpin + Clone + 'static,
{
    type ReadBuffer<'cx> = SocketReadBuffer<'cx, P>;
    type WriteBuffer<'cx> = SocketWriteBuffer<'cx>;

    fn poll_close(&mut self, waker: Waker) -> Poll<Result<()>> {
        use libc::*;

        if unsafe { close(self.fd) } < 0 {
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
                    self.fd,
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
                    self.fd,
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
                recv(self.fd, (*buf).as_mut_ptr() as *mut c_void, buf.len(), 0)
            },
        };

        if len < 0 {
            let e = errno();

            set_errno(e);

            if e.0 == libc::EAGAIN || e.0 == libc::EWOULDBLOCK {
                if let Some(timeout) = &timeout {
                    // second time read .
                    if let Some(last_read_poll_time) = self.last_read_poll_time.take() {
                        let elapsed = last_read_poll_time.elapsed().unwrap();

                        if elapsed >= *timeout {
                            return Poll::Ready(Err(Error::new(
                                ErrorKind::TimedOut,
                                format!("File({}) read timeout", self.fd),
                            )));
                        }
                    } else {
                        // first time read
                        self.last_read_poll_time = Some(SystemTime::now());
                    }
                }

                self.poller
                    .watch_readable_event_once(self.fd, waker, timeout);

                return Poll::Pending;
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
                    self.fd,
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
                send(self.fd, buf.as_ptr() as *const c_void, buf.len(), 0)
            },
        };

        if len < 0 {
            let e = errno();

            set_errno(e);

            if e.0 == libc::EAGAIN || e.0 == libc::EWOULDBLOCK {
                if let Some(timeout) = &timeout {
                    // second time read .
                    if let Some(last_write_poll_time) = self.last_write_poll_time.take() {
                        let elapsed = last_write_poll_time.elapsed().unwrap();

                        if elapsed >= *timeout {
                            return Poll::Ready(Err(Error::new(
                                ErrorKind::TimedOut,
                                format!("File({}) read timeout", self.fd),
                            )));
                        }
                    } else {
                        // first time read
                        self.last_write_poll_time = Some(SystemTime::now());
                    }
                }

                self.poller
                    .watch_writable_event_once(self.fd, waker, timeout);

                return Poll::Pending;
            } else {
                return Poll::Ready(Err(Error::from_raw_os_error(e.0)));
            }
        } else {
            return Poll::Ready(Ok(len as usize));
        }
    }
}
