use std::{
    ffi::c_void,
    io::{Error, Result, Seek, SeekFrom},
    os::windows::prelude::{FromRawHandle, IntoRawHandle, OpenOptionsExt},
    ptr::null_mut,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    task::{Poll, Waker},
};

use crate::{
    io::{EventMessage, EventName, IoReactor, RawFd, ReactorOverlapped},
    ReactorHandle, ReactorHandleSeekable,
};

use winapi::{
    shared::winerror::ERROR_IO_PENDING,
    um::{
        errhandlingapi::GetLastError, fileapi::*, handleapi::*, ioapiset::CreateIoCompletionPort,
    },
    um::{minwinbase::*, winbase::*},
};

use super::sys;

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
            self.reactor.cancel_all(*self.fd);
            CloseHandle(*self.fd);
        }
    }

    fn to_raw_fd(&self) -> RawFd {
        *self.fd as RawFd
    }
}

impl sys::File for Handle {
    fn new<P: Into<std::path::PathBuf>>(
        reactor: IoReactor,
        path: P,
        ops: &mut std::fs::OpenOptions,
    ) -> std::io::Result<Self> {
        let raw_fd = ops
            .custom_flags(FILE_FLAG_OVERLAPPED)
            .open(path.into())?
            .into_raw_handle();

        unsafe {
            let completion_port = reactor.io_handle();

            log::debug!("{:?} {:?}", raw_fd, completion_port);

            let ret = CreateIoCompletionPort(raw_fd, completion_port, 0, 0);

            if ret == null_mut() {
                // release socket resource when this method raise an error.
                CloseHandle(raw_fd);
                return Err(Error::last_os_error());
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
            match event.message? {
                EventMessage::Read(len) => {
                    return Poll::Ready(Ok(len));
                }
                _ => {
                    panic!("Inner error")
                }
            }
        }

        let overlapped = ReactorOverlapped::new_raw(fd, EventName::Read);

        log::trace!("file({:?}) read({})", fd, buffer.len(),);

        unsafe {
            let mut number_of_bytes_read = 0u32;
            let ret = ReadFile(
                fd,
                buffer.as_mut_ptr() as *mut c_void,
                buffer.len() as u32,
                &mut number_of_bytes_read as *mut u32,
                overlapped as *mut OVERLAPPED,
            );

            log::trace!("file({:?}) read({}) result({})", fd, buffer.len(), ret);

            //  operation has completed immediately
            if ret != 0 {
                let _: Box<ReactorOverlapped> = overlapped.into();

                return Poll::Ready(Ok(number_of_bytes_read as usize));
            } else {
                if GetLastError() == ERROR_IO_PENDING {
                    self.reactor
                        .once(fd, EventName::Read, cx.waker().clone(), timeout);

                    return Poll::Pending;
                }

                // Release overlapped
                let _: Box<ReactorOverlapped> = overlapped.into();

                return Poll::Ready(Err(Error::last_os_error()));
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
            match event.message? {
                EventMessage::Write(len) => {
                    return Poll::Ready(Ok(len));
                }
                _ => {
                    panic!("Inner error")
                }
            }
        }

        let overlapped = ReactorOverlapped::new_raw(fd, EventName::Write);

        log::trace!("file({:?}) write({})", fd, buffer.len(),);

        unsafe {
            let mut number_of_bytes_written = 0u32;
            let ret = WriteFile(
                fd,
                buffer.as_ptr() as *mut c_void,
                buffer.len() as u32,
                &mut number_of_bytes_written as *mut u32,
                overlapped as *mut OVERLAPPED,
            );

            log::trace!("file({:?}) write({}) result({})", fd, buffer.len(), ret);

            //  operation has completed immediately
            if ret != 0 {
                let _: Box<ReactorOverlapped> = overlapped.into();

                return Poll::Ready(Ok(number_of_bytes_written as usize));
            } else {
                if GetLastError() == ERROR_IO_PENDING {
                    self.reactor
                        .once(fd, EventName::Write, cx.waker().clone(), timeout);

                    return Poll::Pending;
                }

                // Release overlapped
                let _: Box<ReactorOverlapped> = overlapped.into();

                return Poll::Ready(Err(Error::last_os_error()));
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
            let mut file = std::fs::File::from_raw_handle(fd);

            let offset = file.seek(pos)?;

            // Release handle.
            file.into_raw_handle();

            Poll::Ready(Ok(offset))
        }
    }
}
