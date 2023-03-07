//! Reactor for asynchronous io system, e.g: socket,file or pipe..

use std::{
    collections::HashMap,
    io::{Error, ErrorKind, Result},
    sync::{Arc, Mutex},
    task::Waker,
    time::Duration,
};

pub mod sys;

/// Poll opcode.
#[derive(Debug)]
pub enum PollOpCode {
    /// Poll event to register readable event
    Readable(sys::RawFd),
    /// Poll event to register writable event
    Writable(sys::RawFd),
    /// Poll event to notify readable event
    ReadableReady(sys::RawFd, Option<Error>),
    /// Poll event to notify writable event
    WritableReady(sys::RawFd, Option<Error>),
}

/// System io multiplexer trait.
pub trait SysPoller {
    fn poll_once(&mut self, opcodes: &[PollOpCode], timeout: Duration) -> Result<Vec<PollOpCode>>;
}

#[derive(Debug, Default)]
struct EventWakers {
    readables: HashMap<sys::RawFd, Waker>,
    writables: HashMap<sys::RawFd, Waker>,
}

/// System native io multiplexer wrapper.
#[derive(Debug, Default, Clone)]
pub struct PollerWrapper<P: SysPoller + Clone> {
    wakers: Arc<Mutex<EventWakers>>,
    sys_poller: P,
}

impl<P: SysPoller + Clone> PollerWrapper<P> {
    /// Register a once time watcher of readable event for [`fd`](RawFd)
    pub fn watch_readable_event_once(&mut self, fd: sys::RawFd, waker: Waker, timeout: Duration) {
        self.wakers.lock().unwrap().readables.insert(fd, waker);
    }

    /// Register a once time watcher of writable event for [`fd`](RawFd)
    pub fn watch_writable_event_once(&mut self, fd: sys::RawFd, waker: Waker, timeout: Duration) {
        self.wakers.lock().unwrap().writables.insert(fd, waker);
    }

    /// Poll io events once.
    pub fn poll_once(&mut self, timeout: Duration) -> Result<usize> {
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

        {
            let mut wakers = self.wakers.lock().unwrap();

            for opcode in opcodes {
                match opcode {
                    PollOpCode::ReadableReady(fd, Some(err)) => {
                        wakers.readables.remove(&fd);

                        log::error!("query fd({}) readable status returns error, {}", fd, err);
                    }
                    PollOpCode::ReadableReady(fd, None) => {
                        if let Some(waker) = wakers.readables.remove(&fd) {
                            waker.wake_by_ref();
                            log::error!("fd({}) readable event raised,", fd);
                        }
                    }
                    PollOpCode::WritableReady(fd, Some(err)) => {
                        wakers.writables.remove(&fd);

                        log::error!("query fd({}) writable status returns error, {}", fd, err);
                    }
                    PollOpCode::WritableReady(fd, None) => {
                        if let Some(waker) = wakers.writables.remove(&fd) {
                            waker.wake_by_ref();
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
        }

        unimplemented!()
    }
}

/// Default io multiplexer
pub type Poller = PollerWrapper<sys::SysPoller>;
