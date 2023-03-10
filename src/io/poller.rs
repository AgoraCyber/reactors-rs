use std::{fmt::Debug, io::Result, task::Waker};

#[cfg(target_family = "unix")]
pub type RawFd = std::os::fd::RawFd;
#[cfg(target_family = "windows")]
pub type RawFd = winapi::shared::ntdef::HANDLE;

/// IO multiplexer reactor extension
pub trait Poller {
    /// The poller event type.
    type Event: Event;

    /// Poll io event of [`fd`](RawFd)
    fn poll_io_event(&mut self, fd: RawFd) -> Result<Option<Self::Event>>;

    /// Register a one-time listener([`Waker`]) for [`fd`](RawFd)
    fn once(&mut self, fd: RawFd, r#type: <Self::Event as Event>::Name, waker: Waker);

    /// Cancel all listener of [`fd`](RawFd)
    fn cancel_all(&mut self, fd: RawFd);

    /// Get underlay multiplexer io handle.
    fn io_handle(&self) -> RawFd;
}

/// Io event type trait.
pub trait Event {
    /// Event name type
    type Name: Copy + Send + Sync + Debug + 'static;

    /// Message type
    type Message: Send + Sync + Debug + 'static;

    /// Return event name.
    fn name(&self) -> Self::Name;

    /// Get event message reference.
    fn message(&self) -> &Self::Message;

    /// Destroy self and return event message.
    fn into_message(self) -> Self::Message;
}
