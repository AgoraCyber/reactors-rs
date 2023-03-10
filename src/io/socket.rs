#[cfg_attr(target_family = "windows", path = "socket/socket_win32.rs")]
mod socket;
pub use socket::*;

pub mod tcp;
pub mod udp;

pub mod sys {
    use std::{io::Result, net::SocketAddr, task::Poll, time::Duration};

    use crate::io::{IoReactor, RawFd};

    /// System native socket interface.
    pub trait Socket: Sized {
        /// Create new system raw socket
        fn socket(ip_v4: bool, sock_type: i32, protocol: i32) -> Result<RawFd>;

        /// Create new raw tcp socket
        fn tcp(ip_v4: bool) -> Result<RawFd>;
        /// Create new raw udp socket
        fn udp(ip_v4: bool) -> Result<RawFd>;

        /// Bind socket to [`addr`](SocketAddr)
        fn bind(fd: RawFd, addr: SocketAddr) -> Result<()>;

        /// Stream socket start listen incoming connection.
        fn listen(fd: RawFd) -> Result<()>;

        /// Create new wrapper socket and bind to [`reactor`](IoReactor).
        ///
        /// If this method return an error, the implementation must release the input `fd` resource.
        fn new(ip_v4: bool, fd: RawFd, reactor: IoReactor) -> Result<Self>;

        /// Close native socket
        fn close(&mut self);

        /// Start an async connect operator.
        fn poll_connect(
            self: std::pin::Pin<&mut Self>,
            cx: &mut std::task::Context<'_>,
            remote: SocketAddr,
            timeout: Option<Duration>,
        ) -> Poll<Result<()>>;
    }

    /// Socket [`ReadBuffer`](crate::reactor::ReactorHandle::ReadBuffer)
    pub enum ReadBuffer<'cx> {
        Stream(&'cx mut [u8]),
        Datagram(&'cx mut [u8], &'cx mut Option<SocketAddr>),

        Accept(&'cx mut Option<RawFd>, &'cx mut Option<SocketAddr>),
    }

    /// Socket [`WriteBuffer`](crate::reactor::ReactorHandle::WriteBuffer)
    pub enum WriteBuffer<'cx> {
        Stream(&'cx [u8]),
        Datagram(&'cx [u8], &'cx SocketAddr),
    }
}
