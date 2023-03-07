//! Builtin [`Reactor`](crate::Reactor) implementation for io system

pub mod file;
pub mod poller;
pub mod socket;

#[cfg(target_family = "unix")]
pub(crate) unsafe fn noblock(fd: i32) -> std::io::Result<()> {
    use libc::*;

    let flags = fcntl(fd, F_GETFL);

    if flags < 0 {
        return Err(std::io::Error::last_os_error());
    }

    if fcntl(fd, F_SETFL, flags | O_NONBLOCK) < 0 {
        return Err(std::io::Error::last_os_error());
    }

    Ok(())
}
