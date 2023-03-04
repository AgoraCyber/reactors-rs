#[cfg_attr(target_os = "macos", path = "kqueue.rs")]
#[cfg_attr(target_os = "ios", path = "kqueue.rs")]
#[cfg_attr(target_os = "freebsd", path = "kqueue.rs")]
mod impls;

pub use impls::*;

pub enum PollEvent {
    Readable(i32),
    Writable(i32),
}
