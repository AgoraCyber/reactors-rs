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
#[derive(Debug, Clone, Copy, PartialEq, Hash, Eq)]
pub enum PollRequest {
    /// Poll event to register readable event
    Readable(sys::RawFd),
    /// Poll event to register writable event
    Writable(sys::RawFd),
}

impl Display for PollRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Readable(v) => {
                write!(f, "PollOpCode readable({})", v)
            }
            Self::Writable(v) => {
                write!(f, "PollOpCode writable({})", v)
            }
        }
    }
}

/// Poll response message
#[derive(Debug)]
pub enum PollResponse {
    /// Poll event to notify readable event
    ReadableReady(sys::RawFd, Result<()>),
    /// Poll event to notify writable event
    WritableReady(sys::RawFd, Result<()>),
}

impl Display for PollResponse {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
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
    fn poll_once(
        &mut self,
        opcodes: &[PollRequest],
        timeout: Duration,
    ) -> Result<Vec<PollResponse>>;
}

#[derive(Debug)]
struct EventWakers {
    reqs: HashMap<PollRequest, Waker>,
    resps: HashMap<PollRequest, Result<()>>,
    time_wheel: TimeWheel<PollRequest>,
}

impl EventWakers {
    fn new(steps: u64) -> Self {
        Self {
            reqs: Default::default(),
            resps: Default::default(),
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
    ) -> Result<()> {
        let mut wakers = self.wakers.lock().unwrap();

        let request = PollRequest::Readable(fd);

        if let Some(result) = wakers.resps.remove(&request) {
            return result;
        }

        wakers.reqs.insert(request, waker);

        if let Some(timeout) = timeout {
            let timeout = (timeout.as_millis() / self.tick_duration.as_millis()) as u64;

            wakers.time_wheel.add(timeout, request);
        }

        Ok(())
    }

    /// Register a once time watcher of writable event for [`fd`](RawFd)
    pub fn watch_writable_event_once(
        &mut self,
        fd: sys::RawFd,
        waker: Waker,
        timeout: Option<Duration>,
    ) -> Result<()> {
        let mut wakers = self.wakers.lock().unwrap();

        let request = PollRequest::Writable(fd);

        if let Some(result) = wakers.resps.remove(&request) {
            return result;
        }

        wakers.reqs.insert(request, waker);

        if let Some(timeout) = timeout {
            let mut timeout = (timeout.as_millis() / self.tick_duration.as_millis()) as u64;

            if timeout == 0 {
                timeout = 1;
            }

            wakers.time_wheel.add(timeout, request);
        }

        Ok(())
    }
}

impl<P: SysPoller + Clone> Reactor for PollerReactor<P> {
    /// Poll io events once.
    fn poll_once(&mut self, timeout: Duration) -> Result<usize> {
        let opcodes = {
            let wakers = self.wakers.lock().unwrap();

            let mut opcodes = vec![];

            for (req, _) in &wakers.reqs {
                opcodes.push(*req);
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
                    PollResponse::ReadableReady(fd, result) => {
                        let req = PollRequest::Readable(fd);

                        if let Some(waker) = wakers.reqs.remove(&req) {
                            removed_wakers.push(waker);
                        }

                        wakers.resps.insert(req, result);
                    }

                    PollResponse::WritableReady(fd, result) => {
                        let req = PollRequest::Writable(fd);

                        if let Some(waker) = wakers.reqs.remove(&req) {
                            removed_wakers.push(waker);
                        }

                        wakers.resps.insert(req, result);
                    }
                }
            }

            for _ in 0..steps {
                match wakers.time_wheel.tick() {
                    Poll::Ready(opcodes) => {
                        for req in opcodes {
                            if let Some(waker) = wakers.reqs.remove(&req) {
                                removed_wakers.push(waker);
                            }

                            wakers.resps.insert(
                                req,
                                Err(Error::new(
                                    ErrorKind::TimedOut,
                                    format!("poll request({}) timeout", req),
                                )),
                            );
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
