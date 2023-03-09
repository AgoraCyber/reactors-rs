//! Reactor for asynchronous io system, e.g: socket,file or pipe..

use std::{
    collections::HashMap,
    fmt::{Debug, Display},
    io::{Error, ErrorKind, Result},
    sync::{Arc, Mutex},
    task::{Poll, Waker},
    time::{Duration, SystemTime},
};

use crate::{timewheel::TimeWheel, Reactor};

pub mod sys;

#[cfg(target_family = "unix")]
pub type PollContext = ();
#[cfg(target_family = "unix")]
pub type PollResult = Result<PollContext>;

#[cfg(target_family = "windows")]
#[derive(Debug, Clone)]
pub enum PollContext {
    Accept(sys::RawFd, Box<[u8; 32]>),
    Connect,
    Read(usize),
    RecvFrom(usize, Box<[u8; 16]>),
    SendTo(usize),
    Write(usize),
}
#[cfg(target_family = "windows")]
pub type PollResult = Result<PollContext>;

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
///
#[derive(Debug)]
pub enum PollResponse {
    /// Poll event to notify readable event
    ReadableReady(sys::RawFd, PollResult),
    /// Poll event to notify writable event
    WritableReady(sys::RawFd, PollResult),
}

impl Display for PollResponse {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ReadableReady(v, err) => {
                write!(f, "PollOpCode readable ready({:?}), {:?}", v, err)
            }
            Self::WritableReady(v, err) => {
                write!(f, "PollOpCode writable ready({:?}), {:?}", v, err)
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

    #[cfg(target_family = "windows")]
    fn iocp_handle(&self) -> windows::Win32::Foundation::HANDLE;
}

#[derive(Debug)]
struct EventWakers {
    reqs: HashMap<PollRequest, Waker>,
    resps: HashMap<PollRequest, PollResult>,
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
    /// Create new poller reactor instance with [`SysPoller`] instance.
    pub fn new(sys_poller: P) -> Self {
        Self {
            wakers: Arc::new(Mutex::new(EventWakers::new(3600))),
            sys_poller,
            tick_duration: Duration::from_secs(1),
            last_poll_time: SystemTime::now(),
        }
    }

    #[cfg(target_family = "windows")]
    pub fn iocp_handle(&self) -> windows::Win32::Foundation::HANDLE {
        self.sys_poller.iocp_handle()
    }

    pub fn poll_event(&mut self, fd: sys::RawFd) -> Result<Option<PollContext>> {
        let request = PollRequest::Readable(fd);

        let mut wakers = self.wakers.lock().unwrap();

        if let Some(result) = wakers.resps.remove(&request) {
            return result.map(|c| Some(c));
        }

        Ok(None)
    }

    /// Register a once time watcher of readable event for [`fd`](RawFd)
    pub fn on_event(&mut self, request: PollRequest, waker: Waker, timeout: Option<Duration>) {
        let mut wakers = self.wakers.lock().unwrap();

        wakers.reqs.insert(request, waker);

        if let Some(timeout) = timeout {
            let timeout = (timeout.as_millis() / self.tick_duration.as_millis()) as u64;

            wakers.time_wheel.add(timeout, request);
        }
    }
}

#[cfg(target_family = "unix")]
impl<P: SysPoller + Clone> PollerReactor<P> {
    fn do_poll_once(&mut self, timeout: Duration) -> Result<Vec<PollResponse>> {
        let opcodes = {
            let wakers = self.wakers.lock().unwrap();

            let mut opcodes = vec![];

            for (req, _) in &wakers.reqs {
                opcodes.push(*req);
            }

            opcodes
        };

        self.sys_poller.poll_once(&opcodes, timeout)
    }
}

#[cfg(target_family = "windows")]
impl<P: SysPoller + Clone> PollerReactor<P> {
    fn do_poll_once(&mut self, timeout: Duration) -> Result<Vec<PollResponse>> {
        self.sys_poller.poll_once(&[], timeout)
    }
}

impl<P: SysPoller + Clone> Reactor for PollerReactor<P> {
    /// Poll io events once.
    fn poll_once(&mut self, timeout: Duration) -> Result<usize> {
        let resps = self.do_poll_once(timeout)?;

        let wakers = {
            let elapsed = self.last_poll_time.elapsed().unwrap();

            let steps = (elapsed.as_millis() / self.tick_duration.as_millis()) as u64;

            let mut wakers = self.wakers.lock().unwrap();

            let mut removed_wakers = vec![];

            for opcode in resps {
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

        log::trace!("poll_once({})", wakers.len());

        Ok(wakers.len())
    }
}

/// Default io multiplexer
pub type IoReactor = PollerReactor<sys::SysPoller>;
