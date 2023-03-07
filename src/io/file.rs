#[cfg_attr(target_family = "unix", path = "file/file_posix.rs")]
#[cfg_attr(target_family = "windows", path = "file/file_win32.rs")]
mod impls;
pub use impls::*;

mod extend;
pub use extend::*;

mod opcode;
pub use opcode::*;
