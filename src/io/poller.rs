//! Reactor for asynchronous io system, e.g: socket,file or pipe..

use std::{
    collections::HashMap,
    fmt::Display,
    io::{Error, ErrorKind, Result},
    sync::{Arc, Mutex},
    task::{Poll, Waker},
    time::{Duration, SystemTime},
};

use crate::{timewheel::TimeWheel, Reactor};

pub mod sys;

/// Poll opcode.
#[derive(Debug)]
pub enum PollOpCode {
    /// Poll event to register readable event
    Readable(sys::RawFd),
    /// Poll event to register writable event
    Writable(sys::RawFd),
    /// Poll event to notify readable event
    ReadableReady(sys::RawFd, Result<()>),
    /// Poll event to notify writable event
    WritableReady(sys::RawFd, Result<()>),
}

impl Display for PollOpCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Readable(v) => {
                write!(f, "PollOpCode readable({})", v)
            }
            Self::Writable(v) => {
                write!(f, "PollOpCode writable({})", v)
            }
            Self::ReadableReady(v, err) => {
                write!(f, "PollOpCode readable ready({}), {:?}", v, err)
            }
            Self::WritableReady(v, err) => {
                write!(f, "PollOpCode writable ready({}), {:?}", v, err)
            }
        }
    }
}

/// System io multiplexer trait.
pub trait SysPoller {
    fn poll_once(&mut self, opcodes: &[PollOpCode], timeout: Duration) -> Result<Vec<PollOpCode>>;
}

#[derive(Debug)]
struct EventWakers {
    readables: HashMap<sys::RawFd, Waker>,
    writables: HashMap<sys::RawFd, Waker>,
    time_wheel: TimeWheel<PollOpCode>,
}

impl EventWakers {
    fn new(steps: u64) -> Self {
        Self {
            readables: Default::default(),
            writables: Default::default(),
            time_wheel: TimeWheel::new(steps),
        }
    }
}

/// System native io multiplexer wrapper.
#[derive(Debug, Clone)]
pub struct PollerReactor<P: SysPoller + Clone> {
    wakers: Arc<Mutex<EventWakers>>,
    sys_poller: P,
    tick_duration: Duration,
    last_poll_time: SystemTime,
}

impl<P> Default for PollerReactor<P>
where
    P: SysPoller + Clone + Default,
{
    fn default() -> Self {
        Self {
            wakers: Arc::new(Mutex::new(EventWakers::new(3600))),
            sys_poller: Default::default(),
            tick_duration: Duration::from_secs(1),
            last_poll_time: SystemTime::now(),
        }
    }
}

impl<P: SysPoller + Clone> PollerReactor<P> {
    /// Register a once time watcher of readable event for [`fd`](RawFd)
    pub fn watch_readable_event_once(
        &mut self,
        fd: sys::RawFd,
        waker: Waker,
        timeout: Option<Duration>,
    ) {
        let mut wakers = self.wakers.lock().unwrap();

        wakers.readables.insert(fd, waker);

        if let Some(timeout) = timeout {
            let timeout = (timeout.as_millis() / self.tick_duration.as_millis()) as u64;

            wakers.time_wheel.add(timeout, PollOpCode::Readable(fd));
        }
    }

    /// Register a once time watcher of writable event for [`fd`](RawFd)
    pub fn watch_writable_event_once(
        &mut self,
        fd: sys::RawFd,
        waker: Waker,
        timeout: Option<Duration>,
    ) {
        let mut wakers = self.wakers.lock().unwrap();

        wakers.writables.insert(fd, waker);

        if let Some(timeout) = timeout {
            let mut timeout = (timeout.as_millis() / self.tick_duration.as_millis()) as u64;

            if timeout == 0 {
                timeout = 1;
            }

            wakers.time_wheel.add(timeout, PollOpCode::Writable(fd));
        }
    }
}

impl<P: SysPoller + Clone> Reactor for PollerReactor<P> {
    /// Poll io events once.
    fn poll_once(&mut self, timeout: Duration) -> Result<usize> {
        let opcodes = {
            let wakers = self.wakers.lock().unwrap();

            let mut opcodes = vec![];

            for (fd, _) in &wakers.readables {
                opcodes.push(PollOpCode::Readable(*fd));
            }

            for (fd, _) in &wakers.writables {
                opcodes.push(PollOpCode::Writable(*fd));
            }

            opcodes
        };

        let opcodes = self.sys_poller.poll_once(&opcodes, timeout)?;

        let wakers = {
            let elapsed = self.last_poll_time.elapsed().unwrap();

            let steps = (elapsed.as_millis() / self.tick_duration.as_millis()) as u64;

            let mut wakers = self.wakers.lock().unwrap();

            let mut removed_wakers = vec![];

            for opcode in opcodes {
                match opcode {
                    PollOpCode::ReadableReady(fd, Err(err)) => {
                        wakers.readables.remove(&fd);

                        log::error!("query fd({}) readable status returns error, {}", fd, err);
                    }
                    PollOpCode::ReadableReady(fd, Ok(())) => {
                        if let Some(waker) = wakers.readables.remove(&fd) {
                            removed_wakers.push(waker);
                            log::error!("fd({}) readable event raised,", fd);
                        }
                    }
                    PollOpCode::WritableReady(fd, Err(err)) => {
                        wakers.writables.remove(&fd);

                        log::error!("query fd({}) writable status returns error, {}", fd, err);
                    }
                    PollOpCode::WritableReady(fd, Ok(())) => {
                        if let Some(waker) = wakers.writables.remove(&fd) {
                            removed_wakers.push(waker);
                            log::error!("fd({}) writable event raised,", fd);
                        }
                    }
                    _ => {
                        return Err(Error::new(
                            ErrorKind::InvalidData,
                            format!("Underlay sys poller returns invalid opcode: {:?}", opcode),
                        ));
                    }
                }
            }

            for _ in 0..steps {
                match wakers.time_wheel.tick() {
                    Poll::Ready(opcodes) => {
                        for opcode in opcodes {
                            match opcode {
                                PollOpCode::Readable(fd) => {
                                    if let Some(waker) = wakers.readables.remove(&fd) {
                                        removed_wakers.push(waker);
                                        log::error!("fd({}) read event timeout,", fd);
                                    }
                                }
                                PollOpCode::Writable(fd) => {
                                    if let Some(waker) = wakers.writables.remove(&fd) {
                                        removed_wakers.push(waker);
                                        log::error!("fd({}) writable event timeout,", fd);
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                    Poll::Pending => {}
                }
            }

            removed_wakers
        };

        for waker in &wakers {
            waker.wake_by_ref();
        }

        Ok(wakers.len())
    }
}

/// Default io multiplexer
pub type IoReactor = PollerReactor<sys::SysPoller>;
