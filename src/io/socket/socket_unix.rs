use std::{
    ffi::c_void,
    io::*,
    mem::size_of,
    net::SocketAddr,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    task::Poll,
    time::Duration,
};

use errno::{errno, set_errno};
use libc::*;

use os_socketaddr::OsSocketAddr;

use crate::{
    io::{EventName, IoReactor, RawFd},
    ReactorHandle,
};

use super::sys::{self, ReadBuffer, Socket, WriteBuffer};

/// Socket handle wrapper.
#[derive(Debug, Clone)]
pub struct Handle {
    /// Socket handle bind reactor
    pub reactor: IoReactor,
    /// Socket handle bind os fd.
    pub fd: Arc<i32>,
    /// If this socket is ipv4 familiy
    pub ip_v4: bool,
    /// Close status
    pub closed: Arc<AtomicBool>,
}

impl Handle {
    pub fn to_raw_fd(&self) -> RawFd {
        *self.fd as RawFd
    }
}

impl Drop for Handle {
    fn drop(&mut self) {
        // Only self alive.
        if Arc::strong_count(&self.fd) == 1 {
            self.close();
        }
    }
}

impl sys::Socket for Handle {
    fn bind(fd: RawFd, addr: std::net::SocketAddr) -> Result<()> {
        unsafe {
            let addr: OsSocketAddr = addr.into();

            if bind(fd, addr.as_ptr(), addr.len()) < 0 {
                return Err(Error::last_os_error());
            }
        }

        Ok(())
    }

    fn listen(fd: RawFd) -> Result<()> {
        unsafe {
            let on: c_int = 1;

            let len = size_of::<c_int>() as u32;

            if setsockopt(
                fd,
                SOL_SOCKET,
                SO_REUSEADDR,
                on.to_be_bytes().as_ptr() as *const libc::c_void,
                len,
            ) < 0
            {
                return Err(Error::last_os_error());
            }

            if listen(fd, SOMAXCONN as i32) < 0 {
                return Err(Error::last_os_error());
            } else {
                Ok(())
            }
        }
    }

    fn new(ip_v4: bool, fd: RawFd, mut reactor: IoReactor) -> Result<Self> {
        match reactor.on_open_fd(fd) {
            Err(err) => {
                unsafe { close(fd) };
                return Err(err);
            }
            _ => {}
        }

        Ok(Self {
            reactor,
            fd: Arc::new(fd),
            closed: Default::default(),
            ip_v4,
        })
    }

    fn socket(ip_v4: bool, sock_type: i32, protocol: i32) -> Result<RawFd> {
        let socket = unsafe {
            match ip_v4 {
                true => socket(AF_INET, sock_type, protocol),
                false => socket(AF_INET6, sock_type, protocol),
            }
        };

        if socket < 0 {
            return Err(Error::last_os_error());
        }

        unsafe {
            super::super::noblock(socket)?;
        }

        Ok(socket as RawFd)
    }

    fn close(&mut self) {
        log::trace!("close fd({})", *self.fd);
        self.reactor.on_close_fd(*self.fd);

        unsafe {
            close(*self.fd);
        }
    }

    fn tcp(ip_v4: bool) -> Result<RawFd> {
        Self::socket(ip_v4, SOCK_STREAM, IPPROTO_TCP)
    }

    fn udp(ip_v4: bool) -> Result<RawFd> {
        Self::socket(ip_v4, SOCK_DGRAM, IPPROTO_UDP)
    }

    /// Start an async connect operator.
    #[allow(unused)]
    fn poll_connect(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        remote: SocketAddr,
        timeout: Option<Duration>,
    ) -> Poll<Result<()>> {
        // socket fd
        let fd = self.to_raw_fd();

        if let Some(event) = self.reactor.poll_io_event(fd, EventName::Write)? {
            event.message?;
        }

        let addr: OsSocketAddr = remote.into();

        let len = unsafe { connect(fd, addr.as_ptr(), addr.len()) };

        log::trace!("socket({:?}) connect({})", fd, len);

        if len < 0 {
            let e = errno();

            set_errno(e);

            if e.0 == libc::EAGAIN || e.0 == libc::EWOULDBLOCK || e.0 == libc::EINPROGRESS {
                self.reactor
                    .once(fd, EventName::Write, cx.waker().clone(), timeout);

                return Poll::Pending;
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

#[allow(unused)]
impl ReactorHandle for Handle {
    type ReadBuffer<'cx> = sys::ReadBuffer<'cx>;
    type WriteBuffer<'cx> = sys::WriteBuffer<'cx>;

    fn poll_write<'cx>(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buffer: Self::WriteBuffer<'cx>,
        timeout: Option<std::time::Duration>,
    ) -> std::task::Poll<Result<usize>> {
        match buffer {
            WriteBuffer::Datagram(buff, remote) => {
                self.poll_write_datagram(cx, buff, remote, timeout)
            }
            WriteBuffer::Stream(buff) => self.poll_write_stream(cx, buff, timeout),
        }
    }

    fn poll_close(
        mut self: std::pin::Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<()>> {
        match self
            .closed
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        {
            Err(_) => Poll::Ready(Ok(())),
            _ => {
                self.clone();

                Poll::Ready(Ok(()))
            }
        }
    }

    fn poll_read<'cx>(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buffer: Self::ReadBuffer<'cx>,
        timeout: Option<std::time::Duration>,
    ) -> std::task::Poll<Result<usize>> {
        match buffer {
            ReadBuffer::Accept(fd, remote) => self.poll_accept(cx, fd, remote, timeout),
            ReadBuffer::Datagram(buff, remote) => {
                self.poll_read_datagram(cx, buff, remote, timeout)
            }
            ReadBuffer::Stream(buff) => self.poll_read_stream(cx, buff, timeout),
        }
    }
}

impl Handle {
    fn poll_accept<'cx>(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        conn_fd: &'cx mut Option<RawFd>,
        remote: &'cx mut Option<SocketAddr>,
        timeout: Option<std::time::Duration>,
    ) -> std::task::Poll<Result<usize>> {
        let fd = self.to_raw_fd();

        if let Some(event) = self.reactor.poll_io_event(fd, EventName::Read)? {
            event.message?;
        }

        let mut remote_buff = [0u8; size_of::<sockaddr_in6>()];

        let mut len = remote_buff.len() as u32;

        unsafe {
            let len = accept(
                *self.fd,
                remote_buff.as_mut_ptr() as *mut sockaddr,
                &mut len as *mut u32,
            );

            if len != -1 {
                let addr = OsSocketAddr::copy_from_raw(
                    remote_buff.as_mut_ptr() as *mut sockaddr,
                    len as socklen_t,
                );

                *remote = addr.into_addr();

                *conn_fd = Some(len);

                log::trace!(target:"unix_net","fd({}) accept connection({}) from ({:?})", self.fd, len, remote);

                return Poll::Ready(Ok(0));
            } else {
                let e = errno();

                set_errno(e);

                if e.0 == libc::EAGAIN || e.0 == libc::EWOULDBLOCK {
                    self.reactor
                        .once(fd, EventName::Read, cx.waker().clone(), timeout);

                    return Poll::Pending;
                } else {
                    return Poll::Ready(Err(Error::from_raw_os_error(e.0)));
                }
            }
        }
    }

    fn poll_read_datagram<'cx>(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buff: &'cx mut [u8],
        remote: &'cx mut Option<SocketAddr>,
        timeout: Option<std::time::Duration>,
    ) -> std::task::Poll<Result<usize>> {
        let fd = self.to_raw_fd();

        if let Some(event) = self.reactor.poll_io_event(fd, EventName::Read)? {
            event.message?;
        }

        let mut remote_buff = [0u8; size_of::<sockaddr_in6>()];

        let mut len = remote_buff.len() as u32;

        let len = unsafe {
            recvfrom(
                *self.fd,
                buff.as_ptr() as *mut c_void,
                buff.len(),
                0,
                remote_buff.as_mut_ptr() as *mut sockaddr,
                &mut len as *mut u32,
            )
        };

        if len >= 0 {
            let addr = unsafe {
                OsSocketAddr::copy_from_raw(
                    remote_buff.as_mut_ptr() as *mut sockaddr,
                    len as socklen_t,
                )
            };

            *remote = addr.into_addr();

            log::trace!(target:"unix_net","fd({}) recvfrom({:?}) {}", self.fd, remote, len);

            return Poll::Ready(Ok(len as usize));
        } else {
            let e = errno();

            set_errno(e);

            if e.0 == libc::EAGAIN || e.0 == libc::EWOULDBLOCK {
                self.reactor
                    .once(fd, EventName::Read, cx.waker().clone(), timeout);

                return Poll::Pending;
            } else {
                return Poll::Ready(Err(Error::from_raw_os_error(e.0)));
            }
        }
    }

    fn poll_read_stream<'cx>(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buff: &'cx mut [u8],
        timeout: Option<std::time::Duration>,
    ) -> std::task::Poll<Result<usize>> {
        let fd = self.to_raw_fd();

        if let Some(event) = self.reactor.poll_io_event(fd, EventName::Read)? {
            event.message?;
        }

        let len = unsafe { recv(*self.fd, buff.as_ptr() as *mut c_void, buff.len(), 0) };

        if len >= 0 {
            log::trace!(target:"unix_net","fd({}) recv {}", self.fd, len);

            return Poll::Ready(Ok(len as usize));
        } else {
            let e = errno();

            set_errno(e);

            if e.0 == libc::EAGAIN || e.0 == libc::EWOULDBLOCK {
                self.reactor
                    .once(fd, EventName::Read, cx.waker().clone(), timeout);

                return Poll::Pending;
            } else {
                return Poll::Ready(Err(Error::from_raw_os_error(e.0)));
            }
        }
    }

    fn poll_write_datagram<'cx>(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buff: &'cx [u8],
        remote: &'cx SocketAddr,
        timeout: Option<std::time::Duration>,
    ) -> std::task::Poll<Result<usize>> {
        let fd = self.to_raw_fd();

        if let Some(event) = self.reactor.poll_io_event(fd, EventName::Write)? {
            event.message?;
        }

        let addr: OsSocketAddr = remote.clone().into();

        let len = unsafe {
            sendto(
                *self.fd,
                buff.as_ptr() as *mut c_void,
                buff.len(),
                0,
                addr.as_ptr(),
                addr.len(),
            )
        };

        if len >= 0 {
            log::trace!(target:"unix_net","fd({}) sendto {}", self.fd, len);

            return Poll::Ready(Ok(len as usize));
        } else {
            let e = errno();

            set_errno(e);

            if e.0 == libc::EAGAIN || e.0 == libc::EWOULDBLOCK {
                self.reactor
                    .once(fd, EventName::Write, cx.waker().clone(), timeout);

                return Poll::Pending;
            } else {
                return Poll::Ready(Err(Error::from_raw_os_error(e.0)));
            }
        }
    }

    fn poll_write_stream<'cx>(
        mut self: std::pin::Pin<&mut Self>,
        cx: &std::task::Context<'_>,
        buff: &'cx [u8],
        timeout: Option<std::time::Duration>,
    ) -> std::task::Poll<Result<usize>> {
        let fd = self.to_raw_fd();

        if let Some(event) = self.reactor.poll_io_event(fd, EventName::Write)? {
            event.message?;
        }

        let len = unsafe { send(*self.fd, buff.as_ptr() as *mut c_void, buff.len(), 0) };

        if len >= 0 {
            log::trace!(target:"unix_net","fd({}) send {}", self.fd, len);

            return Poll::Ready(Ok(len as usize));
        } else {
            let e = errno();

            set_errno(e);

            if e.0 == libc::EAGAIN || e.0 == libc::EWOULDBLOCK {
                self.reactor
                    .once(fd, EventName::Write, cx.waker().clone(), timeout);

                return Poll::Pending;
            } else {
                return Poll::Ready(Err(Error::from_raw_os_error(e.0)));
            }
        }
    }
}
