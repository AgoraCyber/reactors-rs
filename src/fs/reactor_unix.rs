use std::{
    ffi::CString,
    io::{self, Result},
    ptr::null_mut,
    task::Poll,
    time::Duration,
};

use errno::{errno, set_errno};
use futures::Future;
use libc::c_void;

use crate::reactor::{unix::UnixReactor, Reactor, ReactorSeekable};

use super::OpenOptions;

#[derive(Clone, Debug)]
pub struct FileHandle(i32, *mut libc::FILE);

#[derive(Clone, Debug)]
pub struct FileReactor(UnixReactor);

impl FileReactor {
    pub fn new() -> Self {
        Self(UnixReactor::new())
    }

    pub fn poll_once(&self, timeout: Duration) -> Result<()> {
        self.0.poll_once(timeout)
    }
}

impl Reactor for FileReactor {
    type Handle = FileHandle;

    type Description = OpenOptions;

    type WriteBuffer<'cx> = &'cx [u8];

    type ReadBuffer<'cx> = &'cx mut [u8];

    type Open<'cx> = Open;

    type Write<'cx> = Write<'cx>;

    type Read<'cx> = Read<'cx>;

    fn open<'a, 'cx>(&'a mut self, description: Self::Description) -> Self::Open<'cx>
    where
        'a: 'cx,
    {
        let open_mode = if !description.append_or_truncate {
            if description.read {
                "w+"
            } else {
                "w"
            }
        } else if description.write {
            if description.read {
                "a+"
            } else {
                "a"
            }
        } else {
            "a+"
        };

        let open_mode = CString::new(open_mode).unwrap();

        let path = CString::new(description.path.to_str().unwrap()).unwrap();

        let handle = unsafe {
            use libc::*;

            let handle = fopen(path.as_ptr(), open_mode.as_ptr());

            let fd = fileno(handle);

            let flags = fcntl(fd, F_GETFL);

            if flags < 0 {
                Err(io::Error::last_os_error())
            } else {
                // set fd to nonblock
                if fcntl(fd, F_SETFL, flags | O_NONBLOCK) < 0 {
                    Err(io::Error::last_os_error())
                } else {
                    if handle == null_mut() {
                        Err(io::Error::last_os_error())
                    } else {
                        Ok(FileHandle(fd, handle))
                    }
                }
            }
        };

        Open(Some(handle))
    }

    /// Close file by [`handle`](FileHandle).
    fn close(&mut self, handle: Self::Handle) -> Result<()> {
        unsafe { libc::fclose(handle.1) };

        Ok(())
    }

    fn read<'a, 'cx>(
        &'a mut self,
        handle: Self::Handle,
        buff: Self::ReadBuffer<'cx>,
    ) -> Self::Read<'cx>
    where
        'a: 'cx,
    {
        Read(handle, buff, self.clone())
    }

    fn write<'a, 'cx>(
        &'a mut self,
        handle: Self::Handle,
        buff: Self::WriteBuffer<'cx>,
    ) -> Self::Write<'cx>
    where
        'a: 'cx,
    {
        Write(handle, buff, self.clone())
    }
}

impl ReactorSeekable for FileReactor {
    type Handle = FileHandle;

    type Seek<'cx> = Seek;
    fn seek<'a, 'cx>(&'a mut self, handle: Self::Handle, pos: std::io::SeekFrom) -> Self::Seek<'cx>
    where
        'a: 'cx,
    {
        let offset = match pos {
            io::SeekFrom::Current(offset) => unsafe {
                libc::fseek(handle.1, offset, libc::SEEK_CUR)
            },
            io::SeekFrom::Start(offset) => unsafe {
                libc::fseek(handle.1, offset as i64, libc::SEEK_SET)
            },
            io::SeekFrom::End(offset) => unsafe { libc::fseek(handle.1, offset, libc::SEEK_END) },
        };

        Seek(offset as u64)
    }
}

pub struct Open(Option<Result<FileHandle>>);

impl Future for Open {
    type Output = Result<FileHandle>;
    fn poll(
        mut self: std::pin::Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        std::task::Poll::Ready(self.0.take().unwrap())
    }
}

pub struct Write<'cx>(FileHandle, &'cx [u8], FileReactor);

impl<'cx> Future for Write<'cx> {
    type Output = Result<usize>;
    fn poll(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        let len = unsafe { libc::write(self.0 .0, self.1.as_ptr() as *const c_void, self.1.len()) };

        if len == 0 {
            let e = errno();

            set_errno(e);

            if e.0 == libc::EAGAIN || e.0 == libc::EWOULDBLOCK {
                let fd = self.0 .0;
                // register event notify
                self.2 .0.event_writable_set(fd, cx.waker().clone());

                log::trace!("write data WOULDBLOCK");

                return Poll::Pending;
            } else {
                return Poll::Ready(Err(io::Error::from_raw_os_error(e.0)));
            }
        } else {
            log::trace!(target:"unix_fs","fd({}) write bytes({})", self.0 .0, len);
            return Poll::Ready(Ok(len as usize));
        }
    }
}

pub struct Read<'cx>(FileHandle, &'cx mut [u8], FileReactor);

impl<'cx> Future for Read<'cx> {
    type Output = Result<usize>;
    fn poll(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        let len =
            unsafe { libc::read(self.0 .0, self.1.as_mut_ptr() as *mut c_void, self.1.len()) };

        if len < 0 {
            let e = errno();

            set_errno(e);

            if e.0 == libc::EAGAIN || e.0 == libc::EWOULDBLOCK {
                let fd = self.0 .0;
                // register event notify
                self.2 .0.event_readable_set(fd, cx.waker().clone());

                return Poll::Pending;
            } else {
                return Poll::Ready(Err(io::Error::from_raw_os_error(e.0)));
            }
        } else {
            log::trace!(target:"unix_fs","fd({}) read bytes({})", self.0 .0, len);
            return Poll::Ready(Ok(len as usize));
        }
    }
}

pub struct Seek(u64);

impl Future for Seek {
    type Output = Result<u64>;
    fn poll(
        self: std::pin::Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        std::task::Poll::Ready(Ok(self.0))
    }
}