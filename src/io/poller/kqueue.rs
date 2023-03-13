use std::{
    io::{Error, Result},
    ptr::null_mut,
    sync::Arc,
    time::Duration,
};

use super::{Event, EventName, Key, RawFd};
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
        let kq_handle = unsafe { libc::kqueue() };

        Ok(Self {
            handle: Arc::new(kq_handle),
        })
    }
    pub fn on_open_fd(&self, fd: RawFd) -> Result<()> {
        log::debug!("add to kevent fd({})", fd);
        let mut evts = [
            kevent {
                ident: fd as usize,
                filter: EVFILT_WRITE,
                flags: EV_ADD,
                fflags: 0,
                data: 0,
                udata: null_mut(),
            },
            kevent {
                ident: fd as usize,
                filter: EVFILT_READ,
                flags: EV_ADD,
                fflags: 0,
                data: 0,
                udata: null_mut(),
            },
        ];

        let ret = unsafe {
            kevent(
                *self.handle,
                evts.as_mut_ptr(),
                2,
                null_mut(),
                0,
                null_mut(),
            )
        };

        if ret < 0 {
            return Err(Error::last_os_error());
        }

        Ok(())
    }

    pub fn on_close_fd(&self, fd: RawFd) -> Result<()> {
        log::debug!("remove from kevent fd({})", fd);
        let mut evts = [
            kevent {
                ident: fd as usize,
                filter: EVFILT_WRITE,
                flags: EV_DELETE,
                fflags: 0,
                data: 0,
                udata: null_mut(),
            },
            kevent {
                ident: fd as usize,
                filter: EVFILT_READ,
                flags: EV_DELETE,
                fflags: 0,
                data: 0,
                udata: null_mut(),
            },
        ];

        let ret = unsafe {
            kevent(
                *self.handle,
                evts.as_mut_ptr(),
                2,
                null_mut(),
                0,
                null_mut(),
            )
        };

        if ret < 0 {
            return Err(Error::last_os_error());
        }

        Ok(())
    }

    pub fn poll_once(&self, keys: &[Key], timeout: Duration) -> Result<Vec<Event>> {
        // let mut changes = Vec::<kevent>::with_capacity(keys.len());

        use libc::*;

        // for key in keys {
        //     let k_event = match key.1 {
        //         EventName::Read => kevent {
        //             ident: key.0 as usize,
        //             filter: EVFILT_READ,
        //             flags: EV_ADD | EV_ONESHOT | EV_ENABLE,
        //             fflags: 0,
        //             data: 0,
        //             udata: null_mut(),
        //         },
        //         EventName::Write => kevent {
        //             ident: key.0 as usize,
        //             filter: EVFILT_WRITE,
        //             flags: EV_ADD | EV_ONESHOT | EV_ENABLE,
        //             fflags: 0,
        //             data: 0,
        //             udata: null_mut(),
        //         },
        //     };

        //     changes.push(k_event);
        // }

        let mut fired_events = vec![unsafe { std::mem::zeroed() }; keys.len()];

        let timeout = libc::timespec {
            tv_sec: timeout.as_secs() as i64,
            tv_nsec: timeout.subsec_nanos() as i64,
        };

        let fired = unsafe {
            libc::kevent(
                *self.handle,
                null_mut(),
                0,
                fired_events.as_mut_ptr(),
                fired_events.len() as i32,
                &timeout,
            )
        };

        if fired < 0 {
            return Err(Error::last_os_error());
        }

        let mut ret = Vec::with_capacity(fired as usize);

        for i in 0..fired {
            let event = &fired_events[i as usize];

            match event.filter {
                EVFILT_READ => {
                    if event.flags & EV_ERROR != 0 {
                        let error = Error::from_raw_os_error(event.data as i32);
                        log::error!(target:"kevent","fd({}) fired error,{}",event.ident as i32,error);

                        ret.push(Event::from_error(
                            Key(event.ident as i32, EventName::Read),
                            error,
                        ))
                    } else {
                        ret.push(Event {
                            key: Key(event.ident as i32, EventName::Read),
                            message: Ok(()),
                        })
                    }
                }
                EVFILT_WRITE => {
                    if event.flags & EV_ERROR != 0 {
                        let error = Error::from_raw_os_error(event.data as i32);
                        log::error!(target:"kevent","fd({}) fired error,{}",event.ident as i32,error);

                        ret.push(Event::from_error(
                            Key(event.ident as i32, EventName::Write),
                            error,
                        ))
                    } else {
                        ret.push(Event {
                            key: Key(event.ident as i32, EventName::Write),
                            message: Ok(()),
                        })
                    }
                }
                _ => {
                    continue;
                }
            }
        }

        log::debug!("raised {:?}", ret);

        Ok(ret)
    }
}
