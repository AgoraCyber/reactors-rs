use std::{
    ffi::CString,
    io::{Error, Result, SeekFrom},
    path::PathBuf,
    ptr::null_mut,
    sync::Arc,
    task::{Poll, Waker},
};

use errno::set_errno;
use futures::task::noop_waker;

use crate::{
    io::poller::{PollerReactor, SysPoller},
    ReactorHandle, ReactorHandleSeekable,
};

#[derive(Clone, Debug)]
pub struct FileHandle<P>
where
    P: SysPoller + Unpin + Clone + 'static,
{
    poller: PollerReactor<P>,
    fd: Arc<i32>,
    file: *mut libc::FILE,
}

impl<P> Drop for FileHandle<P>
where
    P: SysPoller + Unpin + Clone + 'static,
{
    fn drop(&mut self) {
        // Only self
        if Arc::strong_count(&self.fd) == 1 {
            _ = self.poll_close(noop_waker());
        }
    }
}

impl<P> FileHandle<P>
where
    P: SysPoller + Unpin + Clone + 'static,
{
    /// Create file handle with poller and posix [`FILE`](libc::FILE)
    pub fn new(poller: PollerReactor<P>, file: *mut libc::FILE) -> Self {
        Self {
            poller,
            fd: Arc::new(unsafe { libc::fileno(file) }),
            file,
        }
    }

    pub fn create(poller: PollerReactor<P>, path: PathBuf) -> Result<Self> {
        Self::fopen(poller, "w+", path.to_str().unwrap())
    }

    pub fn open(poller: PollerReactor<P>, path: PathBuf) -> Result<Self> {
        Self::fopen(poller, "a+", path.to_str().unwrap())
    }

    fn fopen(poller: PollerReactor<P>, mode: &str, path: &str) -> Result<Self> {
        let open_mode = CString::new(mode).unwrap();

        let path = CString::new(path).unwrap();

        unsafe {
            use libc::*;

            let handle = fopen(path.as_ptr(), open_mode.as_ptr());

            let fd = fileno(handle);

            let flags = fcntl(fd, F_GETFL);

            if flags < 0 {
                Err(Error::last_os_error())
            } else {
                // set fd to nonblock
                if fcntl(fd, F_SETFL, flags | O_NONBLOCK) < 0 {
                    Err(Error::last_os_error())
                } else {
                    if handle == null_mut() {
                        Err(Error::last_os_error())
                    } else {
                        Ok(FileHandle::new(poller, handle))
                    }
                }
            }
        }
    }
}

impl<P> ReactorHandle for FileHandle<P>
where
    P: SysPoller + Unpin + Clone + 'static,
{
    type ReadBuffer<'cx> = &'cx mut [u8];

    type WriteBuffer<'cx> = &'cx [u8];

    fn poll_close(&mut self, _waker: Waker) -> Poll<Result<()>> {
        log::trace!("close file({})", *self.fd);

        if unsafe { libc::fclose(self.file) } < 0 {
            Poll::Ready(Err(Error::last_os_error()))
        } else {
            Poll::Ready(Ok(()))
        }
    }

    fn poll_read<'cx>(
        &mut self,
        buffer: Self::ReadBuffer<'cx>,
        waker: std::task::Waker,
        timeout: Option<std::time::Duration>,
    ) -> std::task::Poll<std::io::Result<usize>> {
        use libc::*;

        unsafe {
            let len = read(*self.fd, buffer.as_mut_ptr() as *mut c_void, buffer.len());

            if len < 0 {
                let e = errno::errno();
                set_errno(e);

                if e.0 == EAGAIN || e.0 == EWOULDBLOCK {
                    return match self
                        .poller
                        .watch_readable_event_once(*self.fd, waker, timeout)
                    {
                        Ok(_) => Poll::Pending,
                        Err(err) => Poll::Ready(Err(err)),
                    };
                } else {
                    return Poll::Ready(Err(Error::from_raw_os_error(e.0)));
                }
            } else {
                return Poll::Ready(Ok(len as usize));
            }
        }
    }

    fn poll_write<'cx>(
        &mut self,
        buffer: Self::WriteBuffer<'cx>,
        waker: std::task::Waker,
        timeout: Option<std::time::Duration>,
    ) -> std::task::Poll<std::io::Result<usize>> {
        use libc::*;

        unsafe {
            let len = write(*self.fd, buffer.as_ptr() as *const c_void, buffer.len());

            if len < 0 {
                let e = errno::errno();
                set_errno(e);

                if e.0 == EAGAIN || e.0 == EWOULDBLOCK {
                    return match self
                        .poller
                        .watch_writable_event_once(*self.fd, waker, timeout)
                    {
                        Ok(_) => Poll::Pending,
                        Err(err) => Poll::Ready(Err(err)),
                    };
                } else {
                    return Poll::Ready(Err(Error::from_raw_os_error(e.0)));
                }
            } else {
                return Poll::Ready(Ok(len as usize));
            }
        }
    }
}

impl<P> ReactorHandleSeekable for FileHandle<P>
where
    P: SysPoller + Unpin + Clone + 'static,
{
    fn seek(
        &mut self,
        pos: SeekFrom,
        _waker: Waker,
        _timeout: Option<std::time::Duration>,
    ) -> std::task::Poll<std::io::Result<u64>> {
        use libc::*;

        unsafe {
            let offset = match pos {
                SeekFrom::Current(offset) => fseek(self.file, offset, libc::SEEK_CUR),
                SeekFrom::Start(offset) => fseek(self.file, offset as i64, libc::SEEK_SET),
                SeekFrom::End(offset) => fseek(self.file, offset, libc::SEEK_END),
            };

            Poll::Ready(Ok(offset as u64))
        }
    }
}
