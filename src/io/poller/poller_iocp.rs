use std::{
    io::{Error, Result},
    sync::Once,
    time::Duration,
};

use windows::Win32::{Foundation::*, Networking::WinSock::*, System::IO::CreateIoCompletionPort};

use crate::io::poller::{PollRequest, PollResponse};

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
    fn poll_once(
        &mut self,
        opcodes: &[PollRequest],
        timeout: Duration,
    ) -> Result<Vec<PollResponse>> {
        unimplemented!()
    }
}
