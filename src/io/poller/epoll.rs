use std::{
    collections::HashMap,
    io::{Error, Result},
    ptr::null_mut,
    sync::Arc,
    time::Duration,
};

use super::{Event, EventName, Key, RawFd};

use errno::{errno, set_errno};
use libc::*;

/// Event for iocp system.
///
#[derive(Clone, Debug)]
pub struct SysPoller {
    handle: Arc<i32>,
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

        Ok(Self {
            handle: Arc::new(handle),
        })
    }
    pub fn on_open_fd(&self, fd: RawFd) -> Result<()> {
        let event = epoll_event {
            events: (EPOLLIN | EPOLLOUT | EPOLLET) as u32,
            u64: fd as u64,
        };

        let ret = unsafe {
            epoll_ctl(
                *self.handle,
                EPOLL_CTL_ADD,
                fd,
                [event].as_ptr() as *mut epoll_event,
            )
        };

        if ret == -1 {
            return Err(Error::last_os_error());
        }

        return Ok(());
    }

    pub fn on_close_fd(&self, fd: RawFd) -> Result<()> {
        let ret = unsafe { epoll_ctl(*self.handle, EPOLL_CTL_DEL, fd, null_mut()) };

        if ret == -1 {
            return Err(Error::last_os_error());
        }

        return Ok(());
    }

    pub fn poll_once(&self, keys: &[Key], timeout: Duration) -> Result<Vec<Event>> {
        let mut fds = HashMap::new();

        for key in keys {
            match key.1 {
                EventName::Read => {
                    fds.entry(key.0)
                        .and_modify(|c| *c = *c | EPOLLIN)
                        .or_insert(EPOLLIN);
                }
                EventName::Write => {
                    fds.entry(key.0)
                        .and_modify(|c| *c = *c | EPOLLOUT)
                        .or_insert(EPOLLOUT);
                }
            }
        }

        for (fd, ops) in fds {
            let event = epoll_event {
                events: ops as u32,
                u64: fd as u64,
            };

            let ret = unsafe {
                epoll_ctl(
                    *self.handle,
                    EPOLL_CTL_MOD,
                    fd,
                    [event].as_ptr() as *mut epoll_event,
                )
            };

            if ret == -1 {
                return Err(Error::last_os_error());
            }
        }

        let fired_events: Vec<epoll_event> = vec![unsafe { std::mem::zeroed() }; keys.len()];

        let fired = unsafe {
            epoll_wait(
                *self.handle,
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
                    key: Key(event.u64 as i32, EventName::Write),
                    message: Ok(()),
                })
            }
        }

        log::trace!("raised {:?}", events);

        Ok(events)
    }
}
