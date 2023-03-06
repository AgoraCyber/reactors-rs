#[cfg_attr(target_family = "unix", path = "reactor_unix.rs")]
#[cfg_attr(target_family = "windows", path = "reactor_windows.rs")]
mod impls;

use std::{io::Result, net::SocketAddr};

#[cfg(target_family = "unix")]
use std::mem::size_of;

pub use impls::*;

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
