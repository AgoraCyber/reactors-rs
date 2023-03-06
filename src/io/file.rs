#[cfg_attr(target_family = "unix", path = "file_posix.rs")]
#[cfg_attr(target_family = "windows", path = "file_win32.rs")]
mod impls;
pub use impls::*;
