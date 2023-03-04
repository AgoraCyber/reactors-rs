use std::{
    future::Future,
    io::{Result, SeekFrom},
};

/// Cross-platform event loop reactor.
pub trait Reactor {
    /// File handle
    type Handle;
    /// File description and open parameters.
    type Description;

    /// Buffer type for file read/write operators.
    type WriteBuffer<'cx>
    where
        Self: 'cx;

    type ReadBuffer<'cx>
    where
        Self: 'cx;

    /// returns by [`open`](Reactor::open) method
    type Open<'cx>: Future<Output = Result<Self::Handle>> + 'cx
    where
        Self: 'cx;

    /// returns by [`close`](Reactor::close) method
    type Close<'cx>: Future<Output = Result<()>> + 'cx
    where
        Self: 'cx;

    /// returns by [`write`](Reactor::write) method
    type Write<'cx>: Future<Output = Result<usize>> + 'cx
    where
        Self: 'cx;

    /// returns by [`read`](Reactor::read) method
    type Read<'cx>: Future<Output = Result<usize>> + 'cx
    where
        Self: 'cx;

    /// returns by [`seek`](Reactor::seek) method
    type Seek<'cx>: Future<Output = Result<usize>> + 'cx
    where
        Self: 'cx;

    /// Open file with description and returns handle.
    fn open<'a, 'cx>(&'a mut self, description: Self::Description) -> Self::Open<'cx>
    where
        'a: 'cx;

    /// Close file by [`handle`](FileHandle).
    fn close<'a, 'cx>(&'a mut self, handle: Self::Handle) -> Self::Close<'cx>
    where
        'a: 'cx;

    /// Write data to file
    fn write<'a, 'cx>(
        &'a mut self,
        handle: Self::Handle,
        buff: Self::WriteBuffer<'cx>,
    ) -> Self::Write<'cx>
    where
        'a: 'cx;

    /// Read data from file
    fn read<'a, 'cx>(
        &'a mut self,
        handle: Self::Handle,
        buff: Self::ReadBuffer<'cx>,
    ) -> Self::Read<'cx>
    where
        'a: 'cx;

    /// Try to seek in file stream.
    ///
    /// # implementation
    ///
    /// If the file type doesn't support seek, should returns error.
    fn seek<'a, 'cx>(&'a mut self, handle: Self::Handle, pos: SeekFrom) -> Self::Seek<'cx>
    where
        'a: 'cx;
}
