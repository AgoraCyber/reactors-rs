use std::{
    io::{Error, Result},
    time::Duration,
};

use super::{Event, EventName, Key};

use errno::{errno, set_errno};
use libc::*;

/// Event for iocp system.
///
#[derive(Clone, Debug)]
pub struct SysPoller {
    handle: i32,
}

impl Drop for SysPoller {
    fn drop(&mut self) {
        if Arc::strong_count(&self.handle) == 1 {
            log::debug!("Close iocp handle({:?})", *self.handle);
            unsafe { close(*self.handle) };
        }
    }
}

impl SysPoller {
    pub fn new() -> Result<Self> {
        let handle = unsafe { libc::epoll_create(1) };

        if -1 == handle {
            return Err(Error::last_os_error());
        }

        Ok(Self { handle })
    }
    pub fn io_handle(&self) -> super::RawFd {
        self.handle
    }

    pub fn poll_once(&self, keys: &[Key], timeout: Duration) -> Result<Vec<Event>> {
        for key in keys {
            let event = match key.1 {
                EventName::Read => epoll_event {
                    events: EPOLLIN as u32,
                    u64: key.0 as u64,
                },
                EventName::Write => epoll_event {
                    events: EPOLLOUT as u32,
                    u64: key.0 as u64,
                },
            };

            let ret = unsafe {
                epoll_ctl(
                    self.handle,
                    EPOLL_CTL_ADD,
                    key.0,
                    [event].as_ptr() as *mut epoll_event,
                )
            };

            if ret == -1 {
                log::debug!("1");
                return Err(Error::last_os_error());
            }
        }

        let fired_events: Vec<epoll_event> = vec![unsafe { std::mem::zeroed() }; keys.len()];

        let fired = unsafe {
            epoll_wait(
                self.handle,
                fired_events.as_ptr() as *mut epoll_event,
                fired_events.len() as i32,
                timeout.as_millis() as i32,
            )
        };

        if fired < 0 {
            let e = errno();

            set_errno(e);

            log::debug!("epoll_wait error({})", e);

            return Err(Error::last_os_error());
        }

        let mut events = Vec::with_capacity(fired as usize);

        for i in 0..fired {
            let event = &fired_events[i as usize];

            if event.events & EPOLLIN as u32 != 0 {
                events.push(Event {
                    key: Key(event.u64 as i32, EventName::Read),
                    message: Ok(()),
                })
            }

            if event.events & EPOLLOUT as u32 != 0 {
                events.push(Event {
                    key: Key(event.u64 as i32, EventName::Read),
                    message: Ok(()),
                })
            }
        }

        log::trace!("raised {:?}", events);

        Ok(events)
    }
}
