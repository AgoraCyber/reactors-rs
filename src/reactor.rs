//! Reactor core traits.
//!

use std::{
    io::{Result, SeekFrom},
    task::{Poll, Waker},
    time::Duration,
};

/// File-based reactor pattern core trait.
pub trait Reactor {
    /// Poll reactor events once.
    fn poll_once(&mut self, duration: Duration) -> Result<usize>;
}

/// Reactor pattern support stream seek
pub trait ReactorHandleSeekable {
    /// Try to seek in file stream.
    fn seek(&mut self, pos: SeekFrom, waker: Waker, timeout: Option<Duration>)
        -> Poll<Result<u64>>;
}

/// Reactor file handle must implement this trait.
pub trait ReactorHandle: Sized {
    /// Buffer type for file write operators.
    type WriteBuffer<'cx>: Unpin
    where
        Self: 'cx;
    /// Buffer type for file read operators
    type ReadBuffer<'cx>: Unpin
    where
        Self: 'cx;

    /// Nonblock write data to this file.
    ///
    /// # parameters
    ///
    /// - `buffer` data buffer to write.
    /// - `waker` Writing task [`waking`](Waker) up handle.
    /// - `timeout` Timeout interval for write operations
    fn poll_write<'cx>(
        &mut self,
        buffer: Self::WriteBuffer<'cx>,
        waker: Waker,
        timeout: Option<Duration>,
    ) -> Poll<Result<usize>>;

    /// Nonblock read data from this file.
    ///
    /// # parameters
    ///
    /// - `buffer` data buffer to write.
    /// - `waker` Reading task [`waking`](Waker) up handle.
    /// - `timeout` Timeout interval for write operations
    fn poll_read<'cx>(
        &mut self,
        buffer: Self::ReadBuffer<'cx>,
        waker: Waker,
        timeout: Option<Duration>,
    ) -> Poll<Result<usize>>;

    ///
    fn poll_close(&mut self, waker: Waker) -> Poll<Result<()>>;
}
