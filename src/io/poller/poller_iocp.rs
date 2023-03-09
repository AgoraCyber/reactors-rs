use std::{
    io::{Error, Result},
    mem::zeroed,
    ptr::null_mut,
    sync::Once,
    time::{Duration, SystemTime},
};

use windows::Win32::{
    Foundation::*,
    Networking::WinSock::*,
    System::IO::{CreateIoCompletionPort, GetQueuedCompletionStatus, OVERLAPPED},
};

use crate::io::poller::{PollRequest, PollResponse};

#[repr(C)]
pub struct PollerOVERLAPPED {
    overlapped: OVERLAPPED,
    request: PollRequest,
}

impl PollerOVERLAPPED {
    pub fn new(request: PollRequest) -> Self {
        Self {
            request,
            overlapped: unsafe { zeroed() },
        }
    }
}

impl Into<*mut OVERLAPPED> for PollerOVERLAPPED {
    fn into(self) -> *mut OVERLAPPED {
        Box::into_raw(Box::new(self)) as *mut OVERLAPPED
    }
}

impl From<*mut OVERLAPPED> for Box<PollerOVERLAPPED> {
    fn from(value: *mut OVERLAPPED) -> Self {
        unsafe { Box::from_raw(value as *mut PollerOVERLAPPED) }
    }
}

/// System poller with iocp backend.
#[derive(Clone, Debug)]
pub struct SysPoller {
    iocp: HANDLE,
}

impl Default for SysPoller {
    fn default() -> Self {
        Self::new().unwrap()
    }
}

impl SysPoller {
    pub fn new() -> Result<Self> {
        static WSA_STARTUP: Once = Once::new();

        WSA_STARTUP.call_once(|| unsafe {
            let mut wsa_data = WSADATA::default();

            let error = WSAStartup(2 << 8 | 2, &mut wsa_data);

            if error != 0 {
                Err::<(), Error>(Error::from_raw_os_error(error)).expect("WSAStartup error");
            }

            log::trace!("WSAStartup success")
        });

        let handle =
            unsafe { CreateIoCompletionPort(INVALID_HANDLE_VALUE, HANDLE::default(), 0, 0)? };

        Ok(Self { iocp: handle })
    }
}

impl crate::io::poller::SysPoller for SysPoller {
    fn poll_once(&mut self, timeout: Duration) -> Result<Vec<PollResponse>> {
        let start_time = SystemTime::now();

        let mut resps = vec![];

        loop {
            unsafe {
                let mut transferred = 0u32;
                let mut completionkey = 0usize;

                let mut overlapped: *mut PollerOVERLAPPED = null_mut();

                let elapsed = start_time.elapsed().unwrap();

                if elapsed >= timeout {
                    break;
                }

                let real_timeout = timeout - elapsed;

                if !GetQueuedCompletionStatus(
                    self.iocp,
                    &mut transferred,
                    &mut completionkey,
                    (&mut overlapped).cast::<*mut OVERLAPPED>(),
                    real_timeout.as_millis() as u32,
                )
                .as_bool()
                {
                    // Maybe completion port handle closed
                    if overlapped == null_mut() {
                        return Err(Error::last_os_error());
                    }

                    let overlapped = Box::from_raw(overlapped);

                    resps.push(
                        overlapped
                            .request
                            .into_response(Err(Error::last_os_error())),
                    );
                } else {
                    let overlapped = Box::from_raw(overlapped);

                    resps.push(overlapped.request.into_response(Ok(())));
                }
            }

            if start_time.elapsed().unwrap() > timeout {
                break;
            }
        }

        log::trace!(
            "poll_once: fired [{}]",
            resps
                .iter()
                .map(|e| e.to_string())
                .collect::<Vec<_>>()
                .join(",")
        );

        Ok(resps)
    }
}
