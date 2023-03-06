#[cfg_attr(target_os = "macos", path = "kqueue.rs")]
#[cfg_attr(target_os = "ios", path = "kqueue.rs")]
#[cfg_attr(target_os = "freebsd", path = "kqueue.rs")]
#[cfg_attr(target_os = "windows", path = "windows.rs")]
mod impls;

use std::{fmt::Display, io::Result};

pub use impls::*;

#[derive(Debug)]
pub enum PollEvent {
    Readable(i32),
    Writable(i32),
}

impl Display for PollEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Readable(v) => {
                write!(f, "PollEvent readable({})", v)
            }
            Self::Writable(v) => {
                write!(f, "PollEvent writable({})", v)
            }
        }
    }
}

pub enum PollEventChanged {
    Readable(i32, Result<()>),
    Writable(i32, Result<()>),
}

impl Display for PollEventChanged {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Readable(v, result) => {
                write!(f, "PollEvent readable({}) changed {:?}", v, result)
            }
            Self::Writable(v, result) => {
                write!(f, "PollEvent writable({}) changed {:?}", v, result)
            }
        }
    }
}
