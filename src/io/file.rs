#[cfg_attr(target_family = "unix", path = "file/file_posix.rs")]
#[cfg_attr(target_family = "windows", path = "file/file_win32.rs")]
mod impls;
pub use impls::*;

pub mod extend;

use super::poller::sys;

pub type File = extend::File<sys::SysPoller>;
