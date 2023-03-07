#[cfg_attr(target_family = "unix", path = "./socket/socket_posix.rs")]
#[cfg_attr(target_family = "windows", path = "./socket/socket_win32.rs")]
mod impls;
pub use impls::*;
pub mod extend;

use super::poller::SysPoller;
use std::net::SocketAddr;

/// Socket [`ReadBuffer`](crate::reactor::ReactorHandle::ReadBuffer)
pub enum SocketReadBuffer<'cx, P>
where
    P: SysPoller + Unpin + Clone + 'static,
{
    Stream(&'cx mut [u8]),
    Datagram(&'cx mut [u8], &'cx mut Option<SocketAddr>),

    Accept(
        &'cx mut Option<SocketHandle<P>>,
        &'cx mut Option<SocketAddr>,
    ),
}

/// Socket [`WriteBuffer`](crate::reactor::ReactorHandle::WriteBuffer)
pub enum SocketWriteBuffer<'cx> {
    Stream(&'cx [u8]),
    Datagram(&'cx [u8], &'cx SocketAddr),
}
