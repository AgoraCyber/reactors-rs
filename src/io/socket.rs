#[cfg_attr(target_family = "unix", path = "socket_posix.rs")]
#[cfg_attr(target_family = "windows", path = "socket_win32.rs")]
mod impls;

pub use impls::*;

pub enum SocketReadBuffer<'cx> {
    Buf(&'cx mut [u8]),
}

pub enum SocketWriteBuffer<'cx> {
    Buf(&'cx [u8]),
}
