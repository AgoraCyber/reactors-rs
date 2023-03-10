#[cfg_attr(target_family = "windows", path = "poller/poller_win32.rs")]
mod os;
pub use os::*;

use std::{
    collections::HashMap,
    fmt::Debug,
    hash::Hash,
    io::{Error, ErrorKind, Result},
    sync::{Arc, Mutex, MutexGuard},
    task::{Poll, Waker},
    time::{Duration, SystemTime},
};

use crate::{timewheel::TimeWheel, Reactor};

#[cfg(target_family = "unix")]
pub type RawFd = std::os::fd::RawFd;
#[cfg(target_family = "windows")]
pub type RawFd = winapi::shared::ntdef::HANDLE;

#[derive(Debug, PartialEq, Hash, Eq, Clone)]
pub struct Key(RawFd, EventName);

/// [`SysPoller`] event type.
#[derive(Debug)]
pub struct Event {
    /// Event key
    pub key: Key,
    /// Event message
    pub message: Result<EventMessage>,
}

impl Event {
    /// Get event bound key.
    pub fn key(&self) -> &Key {
        &self.key
    }

    /// Create event from [`key`](key) and [`error`](Error)
    pub fn from_error(key: Key, err: Error) -> Self {
        Self {
            key,
            message: Err(err),
        }
    }
}

#[derive(Debug)]
struct EventLoop {
    sending: HashMap<Key, Waker>,
    received: HashMap<Key, Event>,
    time_wheel: TimeWheel<Key>,
    last_poll_time: SystemTime,
}

impl EventLoop {
    fn new(steps: u64) -> Self {
        Self {
            sending: Default::default(),
            received: Default::default(),
            time_wheel: TimeWheel::new(steps),
            last_poll_time: SystemTime::now(),
        }
    }
}

/// Io reactor implementation.
#[derive(Clone, Debug)]
pub struct IoReactor {
    poller: SysPoller,
    event_loop: Arc<Mutex<EventLoop>>,
    tick_duration: Duration,
}

impl IoReactor {
    fn poll_timeout(
        event_loop: &mut MutexGuard<EventLoop>,
        tick_duration: &Duration,
    ) -> Vec<Waker> {
        let elapsed = event_loop.last_poll_time.elapsed().unwrap();

        // update last poll time
        event_loop.last_poll_time = SystemTime::now();

        let steps = (elapsed.as_millis() / tick_duration.as_millis()) as u64;

        let mut wakers = vec![];

        for _ in 0..steps {
            if let Poll::Ready(keys) = event_loop.time_wheel.tick() {
                for key in keys {
                    // Get waker
                    if let Some(waker) = event_loop.sending.remove(&key) {
                        wakers.push(waker);
                    }

                    // Insert timeout result
                    event_loop.received.insert(
                        key.clone(),
                        Event::from_error(
                            key.clone(),
                            Error::new(
                                ErrorKind::TimedOut,
                                format!("fd({}) {:?} timeout", key.0 as usize, key.1),
                            ),
                        ),
                    );
                }
            }
        }

        wakers
    }
}

impl IoReactor {
    pub fn new(tick_duration: Duration) -> Result<Self> {
        let poller = SysPoller::new()?;

        Ok(Self {
            poller,
            event_loop: Arc::new(Mutex::new(EventLoop::new(3600))),
            tick_duration,
        })
    }
    pub fn cancel_all(&mut self, fd: super::RawFd) {
        let mut event_loop = self.event_loop.lock().unwrap();

        let mut keys = vec![];

        for (key, _) in &event_loop.sending {
            if key.0 == fd {
                keys.push(key.clone());
            }
        }

        for key in keys {
            event_loop.sending.remove(&key);
        }
    }

    pub fn io_handle(&self) -> super::RawFd {
        self.poller.io_handle()
    }

    pub fn once(
        &mut self,
        fd: super::RawFd,
        name: EventName,
        waker: Waker,
        timeout: Option<Duration>,
    ) {
        let mut event_loop = self.event_loop.lock().unwrap();

        let key = Key(fd, name);

        event_loop.sending.insert(key.clone(), waker);

        if let Some(timeout) = timeout {
            let timeout = (timeout.as_millis() / self.tick_duration.as_millis()) as u64;

            event_loop.time_wheel.add(timeout, key);
        }
    }

    pub fn poll_io_event(&mut self, fd: super::RawFd, name: EventName) -> Result<Option<Event>> {
        let mut event_loop = self.event_loop.lock().unwrap();

        Ok(event_loop.received.remove(&Key(fd, name)))
    }
}

impl Reactor for IoReactor {
    fn poll_once(&mut self, duration: Duration) -> Result<usize> {
        let event_keys = {
            let event_loop = self.event_loop.lock().unwrap();

            let mut keys = vec![];

            for (k, _) in &event_loop.sending {
                keys.push(k.clone());
            }

            keys
        };

        let events = self.poller.poll_once(&event_keys, duration)?;

        let (wakers, timeout_wakers) = {
            let mut wakers = vec![];
            let mut event_loop = self.event_loop.lock().unwrap();

            for event in events {
                if let Some(waker) = event_loop.sending.remove(event.key()) {
                    wakers.push(waker);

                    event_loop.received.insert(event.key().clone(), event);
                }
            }

            let timeout_wakers = Self::poll_timeout(&mut event_loop, &self.tick_duration);

            (wakers, timeout_wakers)
        };

        for waker in &wakers {
            waker.wake_by_ref();
        }

        for waker in &timeout_wakers {
            waker.wake_by_ref();
        }

        Ok(wakers.len() + timeout_wakers.len())
    }
}
