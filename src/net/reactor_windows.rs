use std::{
    io::{Error, Result},
    net::SocketAddr,
    sync::Once,
    time::Duration,
};

use futures::{future::BoxFuture, Future, FutureExt};
use os_socketaddr::OsSocketAddr;
use windows::Win32::Networking::WinSock::{WSAStartup, WSADATA};

use crate::{net::socket::OpenTcpConnect, reactor::Reactor};

use super::{NetOpenOptions, ReadBuffer, WriteBuffer};

pub type SocketFd = windows::Win32::Networking::WinSock::SOCKET;

/// Reactor for windows network events
#[derive(Clone, Debug)]
pub struct NetReactor;

impl NetReactor {
    pub fn new() -> Self {
        static WSA_STARTUP: Once = Once::new();

        WSA_STARTUP.call_once(|| unsafe {
            let mut wsa_data = WSADATA::default();

            let error = WSAStartup(2 << 8 | 2, &mut wsa_data);

            if error != 0 {
                Err::<(), Error>(Error::from_raw_os_error(error)).expect("WSAStartup error");
            }

            log::trace!("WSAStartup success")
        });

        NetReactor
    }

    pub fn poll_once(&mut self, _timeout: Duration) -> Result<()> {
        unimplemented!()
    }

    /// Get the account of io waiting tasks.
    pub fn wakers(&self) -> usize {
        0
    }
}

impl Reactor for NetReactor {
    type Handle = SocketFd;

    type Description = NetOpenOptions;

    type ReadBuffer<'cx> = ReadBuffer<'cx>;

    type WriteBuffer<'cx> = WriteBuffer<'cx>;

    type Open<'cx> = BoxFuture<'static, Result<SocketFd>>;

    type Write<'cx> = Write<'cx>;

    type Read<'cx> = Read<'cx>;

    fn close(&mut self, handle: Self::Handle) -> Result<()> {
        use windows::Win32::Networking::WinSock::*;

        // Ignore close error.
        unsafe { closesocket(handle) };

        Ok(())
    }

    fn open<'a, 'cx>(&'a mut self, description: Self::Description) -> Self::Open<'cx>
    where
        'a: 'cx,
    {
        description.into_fd(self.clone()).boxed()
    }

    fn read<'a, 'cx>(
        &'a mut self,
        handle: Self::Handle,
        buff: Self::ReadBuffer<'cx>,
    ) -> Self::Read<'cx>
    where
        'a: 'cx,
    {
        Read(handle, buff, self.clone())
    }

    fn write<'a, 'cx>(
        &'a mut self,
        handle: Self::Handle,
        buff: Self::WriteBuffer<'cx>,
    ) -> Self::Write<'cx>
    where
        'a: 'cx,
    {
        Write(handle, buff, self.clone())
    }
}

pub struct Write<'cx>(SocketFd, WriteBuffer<'cx>, NetReactor);

#[allow(unused)]
impl<'cx> Future for Write<'cx> {
    type Output = Result<usize>;
    fn poll(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        unimplemented!()
    }
}

pub struct Read<'cx>(SocketFd, ReadBuffer<'cx>, NetReactor);

#[allow(unused)]
impl<'cx> Future for Read<'cx> {
    type Output = Result<usize>;
    fn poll(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        unimplemented!()
    }
}

impl NetOpenOptions {
    pub(crate) fn udp(addr: SocketAddr) -> Result<SocketFd> {
        use windows::Win32::Networking::WinSock::*;

        unsafe { Self::sock(&addr, SOCK_DGRAM, true) }
    }

    pub(crate) fn tcp_listener(addr: SocketAddr) -> Result<SocketFd> {
        use windows::Win32::Networking::WinSock::*;

        unsafe {
            let fd = Self::sock(&addr, SOCK_STREAM, true)?;

            if listen(fd, SOMAXCONN as i32) < 0 {
                return Err(Error::last_os_error());
            }

            Ok(fd)
        }
    }

    pub(crate) fn tcp_connect(
        reactor: NetReactor,
        to: SocketAddr,
        bind_addr: Option<SocketAddr>,
    ) -> Result<OpenTcpConnect> {
        use windows::Win32::Networking::WinSock::*;

        let fd = if let Some(addr) = bind_addr {
            unsafe { Self::sock(&addr, SOCK_STREAM, true)? }
        } else {
            unsafe { Self::sock(&to, SOCK_STREAM, false)? }
        };

        let addr: OsSocketAddr = to.clone().into();

        Ok(OpenTcpConnect(reactor, Some(fd), addr))
    }

    unsafe fn sock(addr: &SocketAddr, ty: u16, bind_addr: bool) -> Result<SocketFd> {
        use windows::Win32::Networking::WinSock::*;

        let fd = match addr {
            SocketAddr::V4(_) => {
                WSASocketW(AF_INET.0 as i32, ty as i32, 0, None, 0, WSA_FLAG_OVERLAPPED)
            }
            SocketAddr::V6(_) => WSASocketW(
                AF_INET6.0 as i32,
                ty as i32,
                0,
                None,
                0,
                WSA_FLAG_OVERLAPPED,
            ),
        };

        if bind_addr {
            if ty == SOCK_STREAM {
                let one = 1i32;

                if setsockopt(
                    fd,
                    SOL_SOCKET as i32,
                    SO_REUSEADDR as i32,
                    Some(&one.to_le_bytes()),
                ) < 0
                {
                    return Err(Error::last_os_error());
                }
            }

            let addr: OsSocketAddr = addr.clone().into();

            if bind(fd, addr.as_ptr() as *const SOCKADDR, addr.len()) < 0 {
                return Err(Error::last_os_error());
            }
        }

        Ok(fd)
    }
}
