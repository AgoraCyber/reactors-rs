#[cfg_attr(target_family = "unix", path = "reactor_unix.rs")]
#[cfg_attr(target_family = "windows", path = "reactor_windows.rs")]
mod impls;

use std::path::PathBuf;

pub use impls::*;

/// File open description
#[derive(Debug, Clone)]
pub struct OpenOptions {
    pub read: bool,
    pub write: bool,
    pub append_or_truncate: bool,
    pub path: PathBuf,
}

#[derive(Default, Debug)]
pub struct OpenOptionsBuilder {
    pub read: bool,
    pub write: bool,
    pub append_or_truncate: bool,
}

impl OpenOptions {
    /// Create new open options
    pub fn build() -> OpenOptionsBuilder {
        OpenOptionsBuilder::default()
    }
}
impl OpenOptionsBuilder {
    pub fn read(mut self, flag: bool) -> Self {
        self.read = flag;

        self
    }

    pub fn write(mut self, flag: bool) -> Self {
        self.write = flag;

        self
    }

    pub fn append(mut self, flag: bool) -> Self {
        self.append_or_truncate = flag;

        self
    }

    pub fn truncate(mut self, flag: bool) -> Self {
        self.append_or_truncate = !flag;

        self
    }

    pub fn path<P: Into<PathBuf>>(self, path: P) -> OpenOptions {
        OpenOptions {
            read: self.read,
            write: self.write,
            append_or_truncate: self.append_or_truncate,
            path: path.into(),
        }
    }
}
