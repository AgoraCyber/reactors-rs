use std::{
    io::{Error, Result},
    mem::size_of,
    net::SocketAddr,
    ptr::null_mut,
    sync::Once,
    time::{Duration, SystemTime},
};

use os_socketaddr::OsSocketAddr;
use winapi::um::{errhandlingapi::GetLastError, ioapiset::*};
use winapi::{shared::ntdef::*, um::minwinbase::OVERLAPPED};
use winapi::{shared::winerror::*, shared::ws2def::WSABUF, um::handleapi::*};
use winapi::{shared::ws2def::SOCKADDR, um::minwinbase::OVERLAPPED_ENTRY};
use winapi::{shared::ws2ipdef::SOCKADDR_IN6, um::winsock2::*};

use super::{Event, Key, RawFd};

/// Event types for IOCP
#[derive(Debug, Clone, PartialEq, Hash, Eq)]
pub enum EventName {
    Connect,
    Accept,
    Read,
    RecvFrom,
    Write,
    SendTo,
}

/// Event message for IOCP
#[derive(Debug, Clone, PartialEq, Hash, Eq)]
pub enum EventMessage {
    Connect,
    Accept(RawFd, Option<SocketAddr>),
    Read(usize),
    RecvFrom(usize, Option<SocketAddr>),
    Write(usize),
    SendTo(usize),
}

/// Overlapped structure used by IOCP system.
#[repr(C)]
#[derive(Clone)]
pub(crate) struct ReactorOverlapped {
    pub overlapped: OVERLAPPED,

    pub fd: RawFd,
    /// For accept socket
    pub accept_fd: RawFd,
    /// Send/Recv buff
    pub buff: [WSABUF; 1],
    /// Used by `AcceptEx`
    pub addrs: [u8; size_of::<SOCKADDR_IN6>() * 2],
    /// Address len
    pub addr_len: i32,
    /// operator name
    pub event_name: EventName,
}

impl ReactorOverlapped {
    /// Create new overlapped structure with all fields zero.
    pub fn new(fd: RawFd, event_name: EventName) -> Self {
        unsafe {
            Self {
                overlapped: std::mem::zeroed(),
                fd,
                addr_len: size_of::<SOCKADDR_IN6>() as i32,
                accept_fd: std::mem::zeroed(),
                buff: std::mem::zeroed(),
                addrs: std::mem::zeroed(),
                event_name,
            }
        }
    }

    /// Create new raw overlapped point.
    pub fn new_raw(fd: RawFd, event_name: EventName) -> *mut Self {
        Box::into_raw(Box::new(Self::new(fd, event_name)))
    }
}

impl From<*mut ReactorOverlapped> for Box<ReactorOverlapped> {
    fn from(value: *mut ReactorOverlapped) -> Self {
        unsafe { Box::from_raw(value) }
    }
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

        let mut events = vec![];

        loop {
            unsafe {
                let elapsed = start_time.elapsed().unwrap();

                if elapsed >= timeout {
                    break;
                }

                let real_timeout = timeout - elapsed;

                let mut removed = 0u32;

                let len = std::cmp::max(keys.len(), 20);

                let mut overlapped_entries: Vec<OVERLAPPED_ENTRY> = vec![std::mem::zeroed(); len];

                let ret = GetQueuedCompletionStatusEx(
                    self.io_handle(),
                    overlapped_entries.as_mut_ptr() as *mut OVERLAPPED_ENTRY,
                    overlapped_entries.len() as u32,
                    &mut removed,
                    real_timeout.as_millis() as u32,
                    0,
                );

                if ret > 0 {
                    log::debug!("iocp status({})", removed);

                    let overlappeds = overlapped_entries[..removed as usize]
                        .into_iter()
                        .map(|o| Box::from_raw((*o).lpOverlapped as *mut ReactorOverlapped))
                        .collect::<Vec<_>>();

                    for o in overlappeds {
                        match o.event_name {
                            EventName::Accept => {
                                log::debug!(
                                    "Internal({}) InternalHigh({})",
                                    o.overlapped.Internal,
                                    o.overlapped.InternalHigh
                                );
                                if o.overlapped.Internal != ERROR_SUCCESS as usize {
                                    events.push(Event {
                                        key: Key(o.fd, EventName::Accept),
                                        message: Err(Error::from_raw_os_error(
                                            o.overlapped.Internal as i32,
                                        )),
                                    })
                                } else {
                                    let addr = OsSocketAddr::copy_from_raw(
                                        o.addrs[size_of::<SOCKADDR_IN6>()..].as_ptr()
                                            as *mut SOCKADDR,
                                        size_of::<SOCKADDR_IN6>() as i32,
                                    );

                                    events.push(Event {
                                        key: Key(o.fd, EventName::Accept),
                                        message: Ok(EventMessage::Accept(o.accept_fd, addr.into())),
                                    })
                                }
                            }
                            EventName::Connect => {
                                if o.overlapped.Internal != ERROR_SUCCESS as usize {
                                    events.push(Event {
                                        key: Key(o.fd, EventName::Connect),
                                        message: Err(Error::from_raw_os_error(
                                            o.overlapped.Internal as i32,
                                        )),
                                    })
                                } else {
                                    events.push(Event {
                                        key: Key(o.fd, EventName::Connect),
                                        message: Ok(EventMessage::Connect),
                                    })
                                }
                            }
                            EventName::Read => {
                                if o.overlapped.Internal != ERROR_SUCCESS as usize {
                                    events.push(Event {
                                        key: Key(o.fd, EventName::Read),
                                        message: Err(Error::from_raw_os_error(
                                            o.overlapped.Internal as i32,
                                        )),
                                    })
                                } else {
                                    events.push(Event {
                                        key: Key(o.fd, EventName::Read),
                                        message: Ok(EventMessage::Read(o.overlapped.InternalHigh)),
                                    })
                                }
                            }
                            EventName::RecvFrom => {
                                if o.overlapped.Internal != ERROR_SUCCESS as usize {
                                    events.push(Event {
                                        key: Key(o.fd, EventName::RecvFrom),
                                        message: Err(Error::from_raw_os_error(
                                            o.overlapped.Internal as i32,
                                        )),
                                    })
                                } else {
                                    let addr = OsSocketAddr::copy_from_raw(
                                        o.addrs[size_of::<SOCKADDR_IN6>()..].as_ptr()
                                            as *mut SOCKADDR,
                                        size_of::<SOCKADDR_IN6>() as i32,
                                    );

                                    events.push(Event {
                                        key: Key(o.fd, EventName::RecvFrom),
                                        message: Ok(EventMessage::RecvFrom(
                                            o.overlapped.InternalHigh,
                                            addr.into(),
                                        )),
                                    })
                                }
                            }
                            EventName::Write => {
                                if o.overlapped.Internal != ERROR_SUCCESS as usize {
                                    events.push(Event {
                                        key: Key(o.fd, EventName::Write),
                                        message: Err(Error::from_raw_os_error(
                                            o.overlapped.Internal as i32,
                                        )),
                                    })
                                } else {
                                    events.push(Event {
                                        key: Key(o.fd, EventName::Write),
                                        message: Ok(EventMessage::Write(o.overlapped.InternalHigh)),
                                    })
                                }
                            }
                            EventName::SendTo => {
                                if o.overlapped.Internal != ERROR_SUCCESS as usize {
                                    events.push(Event {
                                        key: Key(o.fd, EventName::SendTo),
                                        message: Err(Error::from_raw_os_error(
                                            o.overlapped.Internal as i32,
                                        )),
                                    })
                                } else {
                                    events.push(Event {
                                        key: Key(o.fd, EventName::SendTo),
                                        message: Ok(EventMessage::SendTo(
                                            o.overlapped.InternalHigh,
                                        )),
                                    })
                                }
                            }
                        }
                    }
                } else {
                    let e = GetLastError();

                    if e == ERROR_ABANDONED_WAIT_0 {
                        log::info!("iocp poller({:?}) closed", self.iocp);
                        return Ok(vec![]);
                    } else if e == WAIT_TIMEOUT {
                        log::info!("iocp poller({:?}) timeout", self.iocp);
                        continue;
                    } else {
                        return Err(Error::last_os_error());
                    }
                }
            }

            if start_time.elapsed().unwrap() > timeout {
                break;
            }
        }

        log::trace!("raised {:?}", events);

        Ok(events)
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
