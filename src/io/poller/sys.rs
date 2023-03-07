#[cfg_attr(target_os = "macos", path = "poller_kqueue.rs")]
#[cfg_attr(target_os = "ios", path = "poller_kqueue.rs")]
#[cfg_attr(target_os = "bsd", path = "poller_kqueue.rs")]
#[cfg_attr(target_family = "windows", path = "poller_win32.rs")]
mod impls;
pub use impls::*;

#[cfg(target_family = "unix")]
pub type RawFd = std::os::fd::RawFd;
#[cfg(target_family = "windows")]
pub type RawFd = windows::Win32::Foundation::HANDLE;
