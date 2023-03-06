use std::{io::Result, time::Duration};

use futures::Future;
use windows::Win32::Foundation::*;

use crate::reactor::{Reactor, ReactorSeekable};

use super::OpenOptions;

/// Windows FileHandle type define
pub type FileHandle = HANDLE;

#[derive(Clone, Debug)]
pub struct FileReactor;

impl FileReactor {
    pub fn new() -> Self {
        Self
    }

    pub fn poll_once(&self, _timeout: Duration) -> Result<()> {
        unimplemented!()
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

    fn open<'a, 'cx>(&'a mut self, _description: Self::Description) -> Self::Open<'cx>
    where
        'a: 'cx,
    {
        unimplemented!()
    }

    /// Close file by [`handle`](FileHandle).
    fn close(&mut self, handle: Self::Handle) -> Result<()> {
        unsafe { CloseHandle(handle) };

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
    fn seek<'a, 'cx>(
        &'a mut self,
        _handle: Self::Handle,
        _pos: std::io::SeekFrom,
    ) -> Self::Seek<'cx>
    where
        'a: 'cx,
    {
        unimplemented!()
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
        self: std::pin::Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        unimplemented!()
    }
}

pub struct Read<'cx>(FileHandle, &'cx mut [u8], FileReactor);

impl<'cx> Future for Read<'cx> {
    type Output = Result<usize>;
    fn poll(
        self: std::pin::Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        unimplemented!()
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
