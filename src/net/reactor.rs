#[cfg_attr(target_family = "unix", path = "reactor_unix.rs")]
mod impls;

use std::{
    ffi::c_int,
    io::{Error, Result},
    mem::size_of,
    net::SocketAddr,
    os::fd::RawFd,
    task::Poll,
};

use errno::{errno, set_errno};
use futures::Future;
pub use impls::*;
use os_socketaddr::OsSocketAddr;

/// Net fd open options.
#[derive(Clone, Debug)]
pub enum NetOpenOptions {
    /// Attach raw socket fd.
    Raw(RawFd),

    /// Create udp socket with bind [`addr`](SocketAddr)
    Udp(SocketAddr),

    /// Create tcp listener with bind [`addr`](SocketAddr)
    TcpListener(SocketAddr),

    /// Connect [`to`](SocketAddr) remote tcp listener with option bind [`addr`](SocketAddr)
    TcpConnect {
        to: SocketAddr,
        bind: Option<SocketAddr>,
    },
}

#[cfg(target_family = "unix")]
impl NetOpenOptions {
    /// Consume options and create os network fd.
    pub async fn into_fd(self, reactor: NetReactor) -> Result<RawFd> {
        match self {
            Self::Raw(fd) => Ok(fd),
            Self::Udp(addr) => Self::udp(addr),

            Self::TcpListener(addr) => Self::tcp_listener(addr),
            Self::TcpConnect { to, bind } => Self::tcp_connect(reactor, to, bind)?.await,
        }
    }

    fn udp(addr: SocketAddr) -> Result<RawFd> {
        Self::sock(&addr, libc::SOCK_DGRAM, true)
    }

    fn tcp_listener(addr: SocketAddr) -> Result<RawFd> {
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
    ) -> Result<TcpConnect> {
        let fd = if let Some(addr) = bind_addr {
            Self::sock(&addr, libc::SOCK_STREAM, true)?
        } else {
            Self::sock(&to, libc::SOCK_STREAM, false)?
        };

        let addr: OsSocketAddr = to.clone().into();

        Ok(TcpConnect(reactor, fd, addr))
    }

    fn sock(addr: &SocketAddr, ty: c_int, bind_addr: bool) -> Result<RawFd> {
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
                // if ty == SOCK_STREAM {
                //     let one: c_int = 1;

                //     if setsockopt(
                //         fd,
                //         SOL_SOCKET,
                //         SO_REUSEADDR,
                //         one.to_be_bytes().as_ptr() as *const c_void,
                //         size_of::<c_int>() as u32,
                //     ) < 0
                //     {
                //         return Err(Error::last_os_error());
                //     }
                // }

                let addr: OsSocketAddr = addr.clone().into();

                if bind(fd, addr.as_ptr(), addr.len()) < 0 {
                    return Err(Error::last_os_error());
                }
            }

            Ok(fd)
        }
    }
}

#[cfg(target_family = "unix")]
struct TcpConnect(NetReactor, RawFd, OsSocketAddr);

#[cfg(target_family = "unix")]
impl Future for TcpConnect {
    type Output = Result<RawFd>;

    fn poll(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        let fd = self.1;

        unsafe {
            let err_no: c_int = 0;

            let mut len = size_of::<c_int>() as u32;

            if libc::getsockopt(
                fd,
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

            if libc::connect(fd, self.2.as_ptr(), self.2.len()) < 0 {
                let e = errno();

                set_errno(e);

                if e.0 == libc::EAGAIN || e.0 == libc::EWOULDBLOCK || e.0 == libc::EINPROGRESS {
                    // register event notify
                    self.0 .0.event_writable_set(fd, cx.waker().clone());

                    return Poll::Pending;
                } else {
                    return Poll::Ready(Err(Error::from_raw_os_error(e.0)));
                }
            } else {
                return Poll::Ready(Ok(self.1));
            }
        }
    }
}
