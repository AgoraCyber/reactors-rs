use std::{
    io::{Error, ErrorKind, SeekFrom},
    task::{Poll, Waker},
    time::SystemTime,
};

use errno::set_errno;

use crate::{
    io::poller::{PollerWrapper, SysPoller},
    ReactorHandle, ReactorHandleSeekable,
};

#[derive(Clone, Debug)]
pub struct FileHandle<P>
where
    P: SysPoller + Unpin + Clone + 'static,
{
    poller: PollerWrapper<P>,
    fd: i32,
    file: *mut libc::FILE,
    last_read_poll_time: Option<SystemTime>,
    last_write_poll_time: Option<SystemTime>,
}

impl<P> FileHandle<P>
where
    P: SysPoller + Unpin + Clone + 'static,
{
    /// Create file handle with poller and posix [`FILE`](libc::FILE)
    pub fn new(poller: PollerWrapper<P>, file: *mut libc::FILE) -> Self {
        Self {
            poller,
            fd: unsafe { libc::fileno(file) },
            file,
            last_read_poll_time: Default::default(),
            last_write_poll_time: Default::default(),
        }
    }
}

impl<P> ReactorHandle for FileHandle<P>
where
    P: SysPoller + Unpin + Clone + 'static,
{
    type ReadBuffer<'cx> = &'cx mut [u8];

    type WriteBuffer<'cx> = &'cx [u8];

    fn poll_close(&mut self) -> std::io::Result<()> {
        if unsafe { libc::fclose(self.file) } < 0 {
            Err(Error::last_os_error())
        } else {
            Ok(())
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
            let len = read(self.fd, buffer.as_mut_ptr() as *mut c_void, buffer.len());

            if len < 0 {
                let e = errno::errno();
                set_errno(e);

                if e.0 == EAGAIN || e.0 == EWOULDBLOCK {
                    if let Some(timeout) = &timeout {
                        // second time read .
                        if let Some(last_read_poll_time) = self.last_read_poll_time.take() {
                            let elapsed = last_read_poll_time.elapsed().unwrap();

                            if elapsed >= *timeout {
                                return Poll::Ready(Err(Error::new(
                                    ErrorKind::TimedOut,
                                    format!("File({}) read timeout", self.fd),
                                )));
                            }
                        } else {
                            // first time read
                            self.last_read_poll_time = Some(SystemTime::now());
                        }
                    }

                    self.poller
                        .watch_readable_event_once(self.fd, waker, timeout);

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
        &mut self,
        buffer: Self::WriteBuffer<'cx>,
        waker: std::task::Waker,
        timeout: Option<std::time::Duration>,
    ) -> std::task::Poll<std::io::Result<usize>> {
        use libc::*;

        unsafe {
            let len = write(self.fd, buffer.as_ptr() as *const c_void, buffer.len());

            if len < 0 {
                let e = errno::errno();
                set_errno(e);

                if e.0 == EAGAIN || e.0 == EWOULDBLOCK {
                    if let Some(timeout) = &timeout {
                        // second time write .
                        if let Some(last_write_poll_time) = self.last_write_poll_time.take() {
                            let elapsed = last_write_poll_time.elapsed().unwrap();

                            if elapsed >= *timeout {
                                return Poll::Ready(Err(Error::new(
                                    ErrorKind::TimedOut,
                                    format!("File({}) write timeout", self.fd),
                                )));
                            }
                        } else {
                            // first time write
                            self.last_write_poll_time = Some(SystemTime::now());
                        }
                    }

                    self.poller
                        .watch_writable_event_once(self.fd, waker, timeout);

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
