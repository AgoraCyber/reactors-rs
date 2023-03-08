use std::{
    io::{Error, Result},
    ptr::null_mut,
    time::Duration,
};

use crate::io::poller::{PollRequest, PollResponse};

#[derive(Clone, Debug)]
pub struct SysPoller {
    kq_handle: i32,
}

impl Default for SysPoller {
    fn default() -> Self {
        let kq_handle = unsafe { libc::kqueue() };

        Self { kq_handle }
    }
}

impl crate::io::poller::SysPoller for SysPoller {
    fn poll_once(
        &mut self,
        events: &[PollRequest],
        timeout: Duration,
    ) -> Result<Vec<PollResponse>> {
        log::trace!(
            "poll_once: changes [{}]",
            events
                .iter()
                .map(|e| e.to_string())
                .collect::<Vec<_>>()
                .join(",")
        );

        let mut changes = Vec::<kevent>::with_capacity(events.len());

        use libc::*;

        for event in events {
            let k_event = match event {
                PollRequest::Readable(fd) => kevent {
                    ident: *fd as usize,
                    filter: EVFILT_READ,
                    flags: EV_ADD | EV_ONESHOT | EV_ENABLE,
                    fflags: 0,
                    data: 0,
                    udata: null_mut(),
                },
                PollRequest::Writable(fd) => kevent {
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

                        ret.push(PollResponse::ReadableReady(event.ident as i32, Err(error)))
                    } else {
                        ret.push(PollResponse::ReadableReady(event.ident as i32, Ok(())))
                    }
                }
                EVFILT_WRITE => {
                    if event.flags & EV_ERROR != 0 {
                        let error = Error::from_raw_os_error(event.data as i32);
                        log::error!(target:"kevent","fd({}) fired error,{}",event.ident as i32,error);
                        ret.push(PollResponse::WritableReady(event.ident as i32, Err(error)))
                    } else {
                        ret.push(PollResponse::WritableReady(event.ident as i32, Ok(())))
                    }
                }
                _ => {
                    continue;
                }
            }
        }

        log::trace!(
            "poll_once: fired [{}]",
            ret.iter()
                .map(|e| e.to_string())
                .collect::<Vec<_>>()
                .join(",")
        );

        Ok(ret)
    }
}
