#[cfg_attr(target_family = "unix", path = "reactor_unix.rs")]
#[cfg_attr(target_family = "windows", path = "reactor_windows.rs")]
mod impls;

use std::{
    ffi::c_int,
    io::{Error, Result},
    net::SocketAddr,
};

#[cfg(target_family = "unix")]
use std::mem::size_of;

pub use impls::*;

use os_socketaddr::OsSocketAddr;

use super::socket::OpenTcpConnect;

pub enum ReadBuffer<'cx> {
    Accept(&'cx mut Option<SocketFd>, &'cx mut Option<SocketAddr>),
    Stream(&'cx mut [u8]),
    Datagram(&'cx mut [u8], &'cx mut Option<SocketAddr>),
}

pub enum WriteBuffer<'cx> {
    Stream(&'cx [u8]),
    Datagram(&'cx [u8], SocketAddr),
}

/// Net fd open options.
#[derive(Clone, Debug)]
pub enum NetOpenOptions {
    /// Attach raw socket fd.
    Raw(SocketFd),

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

impl NetOpenOptions {
    /// Consume options and create os network fd.
    pub async fn into_fd(self, reactor: NetReactor) -> Result<SocketFd> {
        match self {
            Self::Raw(fd) => Ok(fd),
            Self::Udp(addr) => Self::udp(addr),

            Self::TcpListener(addr) => Self::tcp_listener(addr),
            Self::TcpConnect { to, bind } => Self::tcp_connect(reactor, to, bind)?.await,
        }
    }
}

#[cfg(target_family = "windows")]
impl NetOpenOptions {
    fn udp(addr: SocketAddr) -> Result<SocketFd> {
        use windows::Win32::Networking::WinSock::*;

        unsafe { Self::sock(&addr, SOCK_DGRAM, true) }
    }

    fn tcp_listener(addr: SocketAddr) -> Result<SocketFd> {
        use windows::Win32::Networking::WinSock::*;

        unsafe {
            let fd = Self::sock(&addr, SOCK_STREAM, true)?;

            if listen(fd, SOMAXCONN as i32) < 0 {
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
        use windows::Win32::Networking::WinSock::*;

        let fd = if let Some(addr) = bind_addr {
            unsafe { Self::sock(&addr, SOCK_STREAM, true)? }
        } else {
            unsafe { Self::sock(&to, SOCK_STREAM, false)? }
        };

        let addr: OsSocketAddr = to.clone().into();

        Ok(OpenTcpConnect(reactor, Some(fd), addr))
    }

    unsafe fn sock(addr: &SocketAddr, ty: u16, bind_addr: bool) -> Result<SocketFd> {
        use windows::Win32::Networking::WinSock::*;

        let fd = match addr {
            SocketAddr::V4(_) => {
                WSASocketW(AF_INET.0 as i32, ty as i32, 0, None, 0, WSA_FLAG_OVERLAPPED)
            }
            SocketAddr::V6(_) => WSASocketW(
                AF_INET6.0 as i32,
                ty as i32,
                0,
                None,
                0,
                WSA_FLAG_OVERLAPPED,
            ),
        };

        if bind_addr {
            if ty == SOCK_STREAM {
                let one: c_int = 1;

                if setsockopt(
                    fd,
                    SOL_SOCKET as i32,
                    SO_REUSEADDR as i32,
                    Some(&one.to_le_bytes()),
                ) < 0
                {
                    return Err(Error::last_os_error());
                }
            }

            let addr: OsSocketAddr = addr.clone().into();

            if bind(fd, addr.as_ptr() as *const SOCKADDR, addr.len()) < 0 {
                return Err(Error::last_os_error());
            }
        }

        Ok(fd)
    }
}

#[cfg(target_family = "unix")]
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
