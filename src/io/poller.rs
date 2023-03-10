use std::{
    fmt::Debug,
    hash::Hash,
    io::{Error, Result},
    task::Waker,
};

#[cfg(target_family = "unix")]
pub type RawFd = std::os::fd::RawFd;
#[cfg(target_family = "windows")]
pub type RawFd = winapi::shared::ntdef::HANDLE;

/// IO multiplexer reactor extension
pub trait Poller {
    /// The poller event type.
    type Event: Event + Clone;

    /// Poll io event of [`fd`](RawFd)
    fn poll_io_event(
        &mut self,
        fd: RawFd,
        name: <Self::Event as Event>::Name,
    ) -> Result<Option<Self::Event>>;

    /// Register a one-time listener([`Waker`]) for [`fd`](RawFd)
    fn once(&mut self, fd: RawFd, name: <Self::Event as Event>::Name, waker: Waker);

    /// Cancel all listener of [`fd`](RawFd)
    fn cancel_all(&mut self, fd: RawFd);

    /// Get underlay multiplexer io handle.
    fn io_handle(&self) -> RawFd;
}

/// Io event type trait.
pub trait Event {
    /// Event name type
    type Name: Hash + Eq + Clone + Send + Sync + Debug + 'static;

    /// Message type
    type Message: Send + Sync + Debug + 'static;

    /// Return event name.
    fn name(&self) -> Self::Name;

    /// Get event message reference.
    fn message(&self) -> &Self::Message;

    /// Get event bound key
    fn key(&self) -> &Key<Self::Name>;

    /// Create event from io [`error`](Error)
    fn from_error(error: Error) -> Self;
}

#[derive(Debug, PartialEq, Hash, Eq, Clone)]
pub struct Key<Name>(RawFd, Name)
where
    Name: Hash + Eq + Clone + Send + Sync + Debug + 'static;

pub mod sys;
