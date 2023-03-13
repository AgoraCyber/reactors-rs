use std::{
    ffi::c_void,
    io::{Error, Result, Seek, SeekFrom},
    os::fd::{FromRawFd, IntoRawFd},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    task::{Poll, Waker},
};

use crate::{
    io::{EventName, IoReactor, RawFd},
    ReactorHandle, ReactorHandleSeekable,
};

use super::sys;
use errno::set_errno;
use libc::*;

/// Socket handle wrapper.
#[derive(Debug, Clone)]
pub struct Handle {
    /// Socket handle bind reactor
    pub reactor: IoReactor,
    /// Socket handle bind os fd.
    pub fd: Arc<RawFd>,
    /// Close status
    pub closed: Arc<AtomicBool>,
}

impl Drop for Handle {
    fn drop(&mut self) {
        // Only self alive.
        if Arc::strong_count(&self.fd) == 1 {
            self.close();
        }
    }
}

impl Handle {
    fn close(&mut self) {
        unsafe {
            self.reactor.on_close_fd(*self.fd);
            close(*self.fd);
        }
    }

    fn to_raw_fd(&self) -> RawFd {
        *self.fd as RawFd
    }
}

impl sys::File for Handle {
    fn new<P: Into<std::path::PathBuf>>(
        mut reactor: IoReactor,
        path: P,
        ops: &mut std::fs::OpenOptions,
    ) -> std::io::Result<Self> {
        let raw_fd = ops.open(path.into())?.into_raw_fd();

        unsafe {
            match crate::io::noblock(raw_fd) {
                Ok(_) => {}
                Err(err) => {
                    close(raw_fd);
                    return Err(err);
                }
            }

            match reactor.on_open_fd(raw_fd) {
                Err(err) => {
                    close(raw_fd);
                    return Err(err);
                }
                _ => {}
            }
        }

        let handle = Handle {
            reactor,
            fd: Arc::new(raw_fd),
            closed: Default::default(),
        };

        Ok(handle)
    }
}

impl ReactorHandle for Handle {
    type ReadBuffer<'cx> = &'cx mut [u8];

    type WriteBuffer<'cx> = &'cx [u8];

    fn poll_close(
        mut self: std::pin::Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
    ) -> Poll<Result<()>> {
        match self
            .closed
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        {
            Err(_) => Poll::Ready(Ok(())),
            _ => {
                self.close();

                Poll::Ready(Ok(()))
            }
        }
    }

    fn poll_read<'cx>(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buffer: Self::ReadBuffer<'cx>,
        timeout: Option<std::time::Duration>,
    ) -> std::task::Poll<std::io::Result<usize>> {
        let fd = self.to_raw_fd();

        if let Some(event) = self.reactor.poll_io_event(fd, EventName::Read)? {
            event.message?;
        }

        log::trace!("file({:?}) read({})", fd, buffer.len(),);

        unsafe {
            let len = read(*self.fd, buffer.as_mut_ptr() as *mut c_void, buffer.len());

            if len < 0 {
                let e = errno::errno();

                set_errno(e);

                if e.0 == EAGAIN || e.0 == EWOULDBLOCK {
                    self.reactor
                        .once(fd, EventName::Read, cx.waker().clone(), timeout);
                    return Poll::Pending;
                } else {
                    return Poll::Ready(Err(Error::from_raw_os_error(e.0)));
                }
            } else {
                return Poll::Ready(Ok(len as usize));
            }
        }
    }

    fn poll_write<'cx>(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buffer: Self::WriteBuffer<'cx>,
        timeout: Option<std::time::Duration>,
    ) -> std::task::Poll<std::io::Result<usize>> {
        let fd = self.to_raw_fd();

        if let Some(event) = self.reactor.poll_io_event(fd, EventName::Write)? {
            event.message?;
        }

        log::trace!("file({:?}) read({})", fd, buffer.len(),);

        unsafe {
            let len = write(*self.fd, buffer.as_ptr() as *mut c_void, buffer.len());

            if len < 0 {
                let e = errno::errno();

                set_errno(e);

                if e.0 == EAGAIN || e.0 == EWOULDBLOCK {
                    self.reactor
                        .once(fd, EventName::Write, cx.waker().clone(), timeout);
                    return Poll::Pending;
                } else {
                    return Poll::Ready(Err(Error::from_raw_os_error(e.0)));
                }
            } else {
                return Poll::Ready(Ok(len as usize));
            }
        }
    }
}

impl ReactorHandleSeekable for Handle {
    fn seek(
        &mut self,
        pos: SeekFrom,
        _waker: Waker,
        _timeout: Option<std::time::Duration>,
    ) -> std::task::Poll<std::io::Result<u64>> {
        let fd = self.to_raw_fd();

        unsafe {
            let mut file = std::fs::File::from_raw_fd(fd);

            let offset = file.seek(pos)?;

            // Release handle.
            file.into_raw_fd();

            Poll::Ready(Ok(offset))
        }
    }
}
