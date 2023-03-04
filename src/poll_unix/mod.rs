#[cfg_attr(target_os = "macos", path = "kqueue.rs")]
#[cfg_attr(target_os = "ios", path = "kqueue.rs")]
#[cfg_attr(target_os = "freebsd", path = "kqueue.rs")]
mod impls;

use std::{
    collections::HashMap,
    io::Result,
    sync::{Arc, Mutex},
    task::Waker,
    time::Duration,
};

pub use impls::*;

pub enum PollEvent {
    Readable(i32),
    Writable(i32),
}

#[derive(Debug, Default)]
pub(crate) struct Wakers {
    read_ops: HashMap<i32, Waker>,
    write_ops: HashMap<i32, Waker>,
}

impl Wakers {
    fn append_read(&mut self, fd: i32, waker: Waker) {
        self.read_ops.insert(fd, waker);
    }

    fn append_write(&mut self, fd: i32, waker: Waker) {
        self.write_ops.insert(fd, waker);
    }

    fn to_poll_events(&self) -> Vec<PollEvent> {
        let mut poll_events = vec![];

        for (fd, _) in &self.read_ops {
            poll_events.push(PollEvent::Readable(*fd));
        }

        for (fd, _) in &self.write_ops {
            poll_events.push(PollEvent::Writable(*fd));
        }

        poll_events
    }

    fn remove_fired_wakers(&mut self, events: &[PollEvent]) -> Vec<Waker> {
        let mut wakers = vec![];

        for event in events {
            match event {
                PollEvent::Readable(fd) => {
                    if let Some(waker) = self.read_ops.remove(fd) {
                        wakers.push(waker);
                    }
                }
                PollEvent::Writable(fd) => {
                    if let Some(waker) = self.write_ops.remove(fd) {
                        wakers.push(waker);
                    }
                }
            }
        }

        return wakers;
    }
}

/// Reactor for file io.
#[derive(Clone)]
pub struct UnixReactor {
    poller: UnixPoller,
    pub(crate) wakers: Arc<Mutex<Wakers>>,
}

impl UnixReactor {
    pub fn new() -> Self {
        Self {
            poller: UnixPoller::new(),
            wakers: Default::default(),
        }
    }
    /// Invoke poll procedure once,
    pub fn poll_once(&self, timeout: Duration) -> Result<()> {
        let events = self.wakers.lock().unwrap().to_poll_events();

        let fired = self.poller.poll_once(&events, timeout)?;

        let wakers = self.wakers.lock().unwrap().remove_fired_wakers(&fired);

        for waker in wakers {
            waker.wake_by_ref();
        }

        Ok(())
    }

    pub(crate) fn event_readable_set(&mut self, fd: i32, waker: Waker) {
        self.wakers.lock().unwrap().append_read(fd, waker);
    }

    pub(crate) fn event_writable_set(&mut self, fd: i32, waker: Waker) {
        self.wakers.lock().unwrap().append_write(fd, waker);
    }
}
