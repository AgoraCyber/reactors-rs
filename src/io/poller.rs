//! Reactor for asynchronous io system, e.g: socket,file or pipe..

use crate::reactor::ReactorHandle;

/// System platform native asynchronous IO event trait, only `unix` like system need implement this trait.

pub trait Poller {
    type Handle: ReactorHandle;

    /// Register readable event notification for [`handle`](Poller::Handle)
    #[cfg(target_family = "unix")]
    fn event_readable_set(&mut self, handle: Self::Handle);

    /// Register writable event notification for [`handle`](Poller::Handle)
    #[cfg(target_family = "unix")]
    fn event_writable_set(&mut self, handle: Self::Handle);
}

#[cfg_attr(target_os = "macos", path = "poller_kqueue.rs")]
#[cfg_attr(target_os = "ios", path = "poller_kqueue.rs")]
#[cfg_attr(target_os = "bsd", path = "poller_kqueue.rs")]
#[cfg_attr(target_family = "windows", path = "poller_win32.rs")]
mod impls;
pub use impls::*;
