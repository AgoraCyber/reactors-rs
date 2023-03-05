#[cfg_attr(target_family = "unix", path = "reactor_unix.rs")]
mod impls;

use std::{
    ffi::c_int,
    io::{Error, Result},
    net::SocketAddr,
    os::fd::RawFd,
};

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
    pub async fn into_fd(self) -> Result<RawFd> {
        match self {
            Self::Raw(fd) => Ok(fd),
            Self::Udp(addr) => Self::udp(addr),
            Self::TcpListener(addr) => Self::tcp_listener(addr),
            Self::TcpConnect { to, bind } => Self::tcp_connect(to, bind).await,
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

    async fn tcp_connect(to: SocketAddr, bind_addr: Option<SocketAddr>) -> Result<RawFd> {
        use libc::*;

        unsafe {
            let fd = if let Some(addr) = bind_addr {
                Self::sock(&addr, libc::SOCK_STREAM, true)?
            } else {
                Self::sock(&to, libc::SOCK_STREAM, false)?
            };

            let addr: OsSocketAddr = to.clone().into();

            if connect(fd, addr.as_ptr(), addr.len()) < 0 {
                return Err(Error::last_os_error());
            }

            Ok(fd)
        }
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
                let one = 1u8;

                if setsockopt(
                    fd,
                    SOL_SOCKET,
                    SO_REUSEADDR,
                    one.to_be_bytes().as_ptr() as *const c_void,
                    1,
                ) < 0
                {
                    return Err(Error::last_os_error());
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
