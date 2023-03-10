use winapi::um::winsock2::*;

use crate::{io::Poller, Reactor};

/// Socket handle wrapper.
pub struct Handle<R> {
    /// Socket handle bind reactor
    pub reactor: R,
    /// Socket handle bind os fd.
    pub fd: SOCKET,
}

impl<R> From<(R, SOCKET)> for Handle<R> {
    fn from(value: (R, SOCKET)) -> Self {
        Handle {
            reactor: value.0,
            fd: value.1,
        }
    }
}

impl<R> Handle<R> where R: Reactor + Poller + Clone + 'static {}
