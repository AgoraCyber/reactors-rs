#[cfg_attr(target_family = "windows", path = "file/file_win32.rs")]
mod impls;
pub use impls::*;

pub mod sys {
    use std::{fs::OpenOptions, io::Result, path::PathBuf};

    use crate::io::IoReactor;

    /// System native file interface
    pub trait File: Sized {
        /// Create new system raw file handle.
        fn new<P: Into<PathBuf>>(
            reactor: IoReactor,
            path: P,
            ops: &mut OpenOptions,
        ) -> Result<Self>;
    }
}

mod file;
pub use file::*;
