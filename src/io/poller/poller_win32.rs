use std::{
    io::{Error, Result},
    ptr::null_mut,
    sync::Once,
    time::{Duration, SystemTime},
};

use winapi::shared::ntdef::*;
use winapi::um::handleapi::*;
use winapi::um::ioapiset::*;
use winapi::um::minwinbase::OVERLAPPED_ENTRY;
use winapi::um::winsock2::*;

use super::{Event, Key};

#[derive(Debug, Clone, PartialEq, Hash, Eq)]
pub enum EventName {
    Connect,
}

#[derive(Debug, Clone, PartialEq, Hash, Eq)]
pub enum EventMessage {
    Connect(),
}

/// Event for iocp system.
///
#[derive(Clone, Debug)]
pub struct SysPoller {
    iocp: HANDLE,
}

impl SysPoller {
    pub fn new() -> Result<Self> {
        static WSA_STARTUP: Once = Once::new();

        WSA_STARTUP.call_once(|| unsafe {
            let mut wsa_data: WSADATA = std::mem::zeroed();

            let error = WSAStartup(2 << 8 | 2, &mut wsa_data);

            if error != 0 {
                Err::<(), Error>(Error::from_raw_os_error(error)).expect("WSAStartup error");
            }

            log::trace!("WSAStartup success")
        });

        let handle = unsafe { CreateIoCompletionPort(INVALID_HANDLE_VALUE, null_mut(), 0, 0) };

        if handle == null_mut() {
            return Err(Error::last_os_error());
        }

        Ok(Self { iocp: handle })
    }
    pub fn io_handle(&self) -> super::RawFd {
        self.iocp
    }

    pub fn poll_once(&self, keys: &[Key], timeout: Duration) -> Result<Vec<Event>> {
        let start_time = SystemTime::now();

        loop {
            unsafe {
                let elapsed = start_time.elapsed().unwrap();

                if elapsed >= timeout {
                    break;
                }

                let real_timeout = timeout - elapsed;

                let mut bytes_transferred = 0u32;

                let len = std::cmp::max(keys.len(), 20);

                let mut overlapped_entries = vec![std::mem::zeroed(); len];

                GetQueuedCompletionStatusEx(
                    self.io_handle(),
                    overlapped_entries.as_mut_ptr() as *mut OVERLAPPED_ENTRY,
                    overlapped_entries.len() as u32,
                    &mut bytes_transferred,
                    real_timeout.as_millis() as u32,
                    0,
                );
            }

            if start_time.elapsed().unwrap() > timeout {
                break;
            }
        }

        Ok(vec![])
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::SysPoller;

    #[test]
    fn test_poll_one() {
        _ = pretty_env_logger::try_init();

        let poller = SysPoller::new().unwrap();

        poller.poll_once(&[], Duration::from_secs(1)).unwrap();
    }
}
