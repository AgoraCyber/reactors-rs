use std::{
    future::Future,
    io::{Result, SeekFrom},
    time::Duration,
};

/// Reactor pattern core trait.
pub trait Reactor {
    /// Session handle
    type Handle: Unpin;

    /// Session open code.
    type OpCode: Unpin;

    /// Buffer type for file read/write operators.
    type WriteBuffer<'cx>: Unpin
    where
        Self: 'cx;

    type ReadBuffer<'cx>: Unpin
    where
        Self: 'cx;

    /// returns by [`open`](Reactor::open) method
    type Open<'cx>: Future<Output = Result<Self::Handle>> + Unpin + 'cx
    where
        Self: 'cx;

    /// returns by [`write`](Reactor::write) method
    type Write<'cx>: Future<Output = Result<usize>> + Unpin + 'cx
    where
        Self: 'cx;

    /// returns by [`read`](Reactor::read) method
    type Read<'cx>: Future<Output = Result<usize>> + Unpin + 'cx
    where
        Self: 'cx;

    /// Open file with description and returns handle.
    fn open<'a, 'cx>(&'a mut self, opcode: Self::OpCode) -> Self::Open<'cx>
    where
        'a: 'cx;

    /// Close file by [`handle`](FileHandle).
    fn close(&mut self, handle: Self::Handle) -> Result<()>;

    /// Write data to file
    fn write<'a, 'cx>(
        &'a mut self,
        handle: Self::Handle,
        buff: Self::WriteBuffer<'cx>,
        timeout: Option<Duration>,
    ) -> Self::Write<'cx>
    where
        'a: 'cx;

    /// Read data from file
    ///
    /// # parameter
    fn read<'a, 'cx>(
        &'a mut self,
        handle: Self::Handle,
        buff: Self::ReadBuffer<'cx>,
        timeout: Option<Duration>,
    ) -> Self::Read<'cx>
    where
        'a: 'cx;
}

pub trait ReactorSeekable {
    /// File handle
    type Handle;

    /// returns by [`seek`](Reactor::seek) method
    type Seek<'cx>: Future<Output = Result<u64>> + 'cx
    where
        Self: 'cx;

    /// Try to seek in file stream.
    ///
    /// # implementation
    ///
    /// If the file type doesn't support seek, should returns error.
    fn seek<'a, 'cx>(&'a mut self, handle: Self::Handle, pos: SeekFrom) -> Self::Seek<'cx>
    where
        'a: 'cx;
}
