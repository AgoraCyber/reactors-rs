use std::{
    io::{self, Result},
    ptr::null_mut,
    time::Duration,
};

use super::PollEvent;

#[derive(Clone, Debug)]
pub struct UnixPoller {
    kq_handle: i32,
}

impl UnixPoller {
    pub fn new() -> Self {
        let kq_handle = unsafe { libc::kqueue() };

        Self { kq_handle }
    }

    pub fn poll_once(&self, events: &[PollEvent], timeout: Duration) -> Result<Vec<PollEvent>> {
        let mut changes = Vec::<kevent>::with_capacity(events.len());

        use libc::*;

        for event in events {
            let k_event = match event {
                PollEvent::Readable(fd) => kevent {
                    ident: *fd as usize,
                    filter: EVFILT_WRITE,
                    flags: EV_ADD | EV_ONESHOT | EV_ENABLE,
                    fflags: 0,
                    data: 0,
                    udata: null_mut(),
                },
                PollEvent::Writable(fd) => kevent {
                    ident: *fd as usize,
                    filter: EVFILT_WRITE,
                    flags: EV_ADD | EV_ONESHOT | EV_ENABLE,
                    fflags: 0,
                    data: 0,
                    udata: null_mut(),
                },
            };

            changes.push(k_event);
        }

        let mut fired_events = vec![
            kevent {
                ident: 0,
                filter: 0,
                flags: 0,
                fflags: 0,
                data: 0,
                udata: null_mut(),
            };
            events.len()
        ];

        let timeout = libc::timespec {
            tv_sec: timeout.as_secs() as i64,
            tv_nsec: timeout.subsec_nanos() as i64,
        };

        let fired = unsafe {
            libc::kevent(
                self.kq_handle,
                changes.as_mut_ptr(),
                changes.len() as i32,
                fired_events.as_mut_ptr(),
                fired_events.len() as i32,
                &timeout,
            )
        };

        if fired < 0 {
            return Err(io::Error::last_os_error());
        }

        let mut ret = Vec::with_capacity(fired as usize);

        for i in 0..fired {
            let event = &fired_events[i as usize];

            match event.filter {
                EVFILT_READ => {
                    if event.flags & EV_ERROR != 0 {
                        log::error!(target:"kevent","fd({}) fired error,{}",event.ident as i32,io::Error::from_raw_os_error(event.data as i32));
                    } else {
                        ret.push(PollEvent::Readable(event.ident as i32))
                    }
                }
                EVFILT_WRITE => {
                    if event.flags & EV_ERROR != 0 {
                        log::error!(target:"kevent","fd({}) fired error,{}",event.ident as i32,io::Error::from_raw_os_error(event.data as i32));
                    } else {
                        ret.push(PollEvent::Writable(event.ident as i32))
                    }
                }
                _ => {
                    continue;
                }
            }
        }

        Ok(ret)
    }
}
