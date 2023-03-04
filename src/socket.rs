#[cfg_attr(target_family = "unix", path = "socket_unix.rs")]
mod impls;
pub use impls::*;

#[derive(Clone, Debug)]
pub enum SockOpenOps {}
