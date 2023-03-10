//! Default [`Poller`](super::Poller) + [`Reactor`] implementation that using system io mutliplexer api.
//!

use std::{
    collections::HashMap,
    fmt::Debug,
    io::{Error, ErrorKind, Result},
    sync::{Arc, Mutex, MutexGuard},
    task::{Poll, Waker},
    time::{Duration, SystemTime},
};

use crate::{timewheel::TimeWheel, Reactor};

use super::{Event, Key, Poller};

/// System io multiplexer trait.
pub trait SysPoller {
    /// The poller event type.
    type Event: Event + Clone;

    fn poll_once(
        &mut self,
        event_keys: &[Key<<Self::Event as Event>::Name>],
        duration: Duration,
    ) -> Result<Vec<Self::Event>>;

    fn io_handle(&self) -> super::RawFd;
}

#[derive(Debug)]
struct EventLoop<E: Event> {
    sending: HashMap<Key<E::Name>, Waker>,
    received: HashMap<Key<E::Name>, E>,
    time_wheel: TimeWheel<Key<E::Name>>,
    last_poll_time: SystemTime,
}

/// Io reactor implementation.
#[derive(Clone)]
pub struct IoReactor<P: SysPoller + Clone> {
    poller: P,
    event_loop: Arc<Mutex<EventLoop<P::Event>>>,
    tick_duration: Duration,
}

impl<P: SysPoller + Clone> IoReactor<P> {
    fn poll_timeout(
        event_loop: &mut MutexGuard<EventLoop<P::Event>>,
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
                        <P::Event as Event>::from_error(Error::new(
                            ErrorKind::TimedOut,
                            format!("fd({}) {:?} timeout", key.0 as usize, key.1),
                        )),
                    );
                }
            }
        }

        wakers
    }
}

impl<P: SysPoller + Clone> Poller for IoReactor<P> {
    type Event = P::Event;

    fn cancel_all(&mut self, fd: super::RawFd) {
        let mut event_loop = self.event_loop.lock().unwrap();

        {
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
    }

    fn io_handle(&self) -> super::RawFd {
        self.poller.io_handle()
    }

    fn once(&mut self, fd: super::RawFd, name: <Self::Event as Event>::Name, waker: Waker) {
        let mut event_loop = self.event_loop.lock().unwrap();

        event_loop.sending.insert(Key(fd, name), waker);
    }

    fn poll_io_event(
        &mut self,
        fd: super::RawFd,
        name: <Self::Event as Event>::Name,
    ) -> Result<Option<Self::Event>> {
        let mut event_loop = self.event_loop.lock().unwrap();

        Ok(event_loop.received.remove(&Key(fd, name)))
    }
}

impl<P: SysPoller + Clone> Reactor for IoReactor<P> {
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

#[cfg_attr(target_family = "windows", path = "sys/poller_win32.rs")]
mod impls;
pub use impls::*;
