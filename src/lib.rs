#![doc = include_str!("../README.md")]

pub mod file;
pub mod reactor;

#[cfg_attr(target_family = "unix", path = "poll_unix/mod.rs")]
pub mod poll_unix;
