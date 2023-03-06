pub mod reactor;
pub mod socket;

#[cfg(feature = "singleton")]
mod global {

    use std::{net::SocketAddr, time::Duration};

    use once_cell::sync::Lazy;

    use crate::reactor::Reactor;

    use super::{
        reactor::{NetOpenOptions, NetReactor},
        socket::{OpenTcpListener, OpenTcpSocket, UdpSocketOpen},
    };

    static INSTANCE_REACTOR: Lazy<NetReactor> = Lazy::new(|| {
        let reactor = NetReactor::new();
        let mut loop_reactor = reactor.clone();

        std::thread::spawn(move || loop {
            match loop_reactor.poll_once(Duration::from_millis(500)) {
                Err(err) => {
                    log::error!(target:"singleton_net_reactor","poll_once error: {}",err);
                    return;
                }
                _ => {}
            }
        });

        reactor
    });

    /// Start a new tcp server listener and bind to [`addr`](SocketAddr)
    pub fn tcp_listen(addr: SocketAddr) -> OpenTcpListener {
        let opts = NetOpenOptions::TcpListener(addr);

        let mut reactor = INSTANCE_REACTOR.clone();

        let fut = reactor.open(opts);

        OpenTcpListener(reactor, fut, addr)
    }

    /// Create new tcp client and connect to remote endpoint.
    pub fn tcp_connect(to: SocketAddr, bind_addr: Option<SocketAddr>) -> OpenTcpSocket {
        let opts = NetOpenOptions::TcpConnect {
            to,
            bind: bind_addr,
        };

        let mut reactor = INSTANCE_REACTOR.clone();

        let fut = reactor.open(opts);

        OpenTcpSocket(reactor, fut, to)
    }

    /// Create new tcp client and connect to remote endpoint.
    pub fn udp(bind_addr: SocketAddr, buff_size: usize) -> UdpSocketOpen {
        let opts = NetOpenOptions::Udp(bind_addr);

        let mut reactor = INSTANCE_REACTOR.clone();

        let fut = reactor.open(opts);

        UdpSocketOpen(reactor, fut, buff_size)
    }
}

#[cfg(feature = "singleton")]
pub use global::*;

#[cfg(test)]
mod tests {

    use std::{io::ErrorKind, task::Poll, time::Duration};

    use futures::{AsyncReadExt, AsyncWriteExt, FutureExt, SinkExt, StreamExt};
    use futures_test::{assert_stream_next, assert_stream_pending, task::noop_context};

    use super::{
        reactor::NetReactor,
        socket::{TcpSocketReactor, UdpSocketReactor},
    };

    #[futures_test::test]
    async fn test_udp() {
        _ = pretty_env_logger::try_init();

        let mut reactor = NetReactor::new();

        let server_addr = "127.0.0.1:1812".parse().unwrap();

        let client_addr = "127.0.0.1:1813".parse().unwrap();

        let mut udp_server = reactor.udp(server_addr, 1024).await.unwrap();

        let mut udp_client = reactor.udp(client_addr, 1024).await.unwrap();

        assert_stream_pending!(udp_server);

        let mut udp_srever = udp_server.map(|c| c.unwrap());

        for i in 0..10 {
            let buff = format!("hello world {}", i).as_bytes().to_vec();

            udp_client
                .send((buff.clone(), server_addr.clone()))
                .await
                .unwrap();

            while reactor.wakers() != 0 {
                reactor.poll_once(Duration::from_secs(1)).unwrap();
            }

            log::debug!("loop");

            assert_stream_next!(udp_srever, (buff, client_addr));
        }
    }

    #[futures_test::test]
    async fn test_tcp() {
        _ = pretty_env_logger::try_init();

        let mut reactor = NetReactor::new();

        let server_addr = "127.0.0.1:1812".parse().unwrap();
        // let client_addr = "127.0.0.1:1813".parse().unwrap();

        let mut listener = reactor.listen(server_addr).await.unwrap();

        let mut ctx = noop_context();
        let mut accept = listener.accept().boxed();

        match accept.poll_unpin(&mut ctx) {
            Poll::Pending => {}
            _ => {
                assert!(false, "expect pending")
            }
        }

        let mut connection = reactor.connect(server_addr, None).boxed();

        match connection.poll_unpin(&mut ctx) {
            Poll::Pending => {}
            _ => {
                assert!(false, "expect pending")
            }
        }

        while reactor.wakers() != 0 {
            reactor.poll_once(Duration::from_secs(1)).unwrap();
        }

        let mut server_connection = accept.await.unwrap();

        let mut client_connection = connection.await.unwrap();

        let write_all = client_connection.write_all(b"hello world").boxed();

        let mut buff = [0u8; 11];

        let read_exact = server_connection.read_exact(&mut buff).boxed();

        while reactor.wakers() != 0 {
            reactor.poll_once(Duration::from_secs(1)).unwrap();
        }

        write_all.await.unwrap();

        read_exact.await.unwrap();

        assert_eq!(&buff, b"hello world");

        client_connection.close().await.unwrap();

        let read_exact = server_connection.read_exact(&mut buff).boxed();

        while reactor.wakers() != 0 {
            reactor.poll_once(Duration::from_secs(1)).unwrap();
        }

        match read_exact.await {
            Ok(_) => {
                assert!(false, "expect eof")
            }
            Err(err) => {
                assert_eq!(err.kind(), ErrorKind::UnexpectedEof);
            }
        }
    }
}
