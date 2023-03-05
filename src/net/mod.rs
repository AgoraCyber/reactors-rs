pub mod reactor;
pub mod socket;

#[cfg(feature = "singleton")]
mod global {

    use std::{net::SocketAddr, time::Duration};

    use once_cell::sync::Lazy;

    use super::{
        reactor::{NetOpenOptions, NetReactor},
        socket::{SocketOpen, TcpSocket, UdpSocketOpen},
    };

    static INSTANCE_REACTOR: Lazy<NetReactor> = Lazy::new(|| {
        let reactor = NetReactor::new();
        let loop_reactor = reactor.clone();

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
    pub fn tcp_listen(addr: SocketAddr) -> SocketOpen<TcpSocket> {
        SocketOpen::<TcpSocket>(
            INSTANCE_REACTOR.clone(),
            NetOpenOptions::TcpListener(addr),
            Default::default(),
        )
    }

    /// Create new tcp client and connect to remote endpoint.
    pub fn tcp_connect(to: SocketAddr, bind_addr: Option<SocketAddr>) -> SocketOpen<TcpSocket> {
        SocketOpen::<TcpSocket>(
            INSTANCE_REACTOR.clone(),
            NetOpenOptions::TcpConnect {
                to,
                bind: bind_addr,
            },
            Default::default(),
        )
    }

    /// Create new tcp client and connect to remote endpoint.
    pub fn udp(bind_addr: SocketAddr, buff_size: usize) -> UdpSocketOpen {
        UdpSocketOpen(
            INSTANCE_REACTOR.clone(),
            NetOpenOptions::Udp(bind_addr),
            buff_size,
        )
    }
}

#[cfg(feature = "singleton")]
pub use global::*;

#[cfg(test)]
mod tests {

    use std::{task::Poll, time::Duration};

    use futures::{FutureExt, SinkExt, StreamExt};
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
            log::debug!("loop {}", i);
            let buff = format!("hello world {}", i).as_bytes().to_vec();

            udp_client
                .send((buff.clone(), server_addr.clone()))
                .await
                .unwrap();

            reactor.poll_once(Duration::from_millis(200)).unwrap();

            assert_stream_next!(udp_srever, (buff, client_addr));
        }
    }

    #[futures_test::test]
    async fn test_tcp() {
        _ = pretty_env_logger::try_init();

        let mut reactor = NetReactor::new();

        let server_addr = "127.0.0.1:1812".parse().unwrap();
        let client_addr = "127.0.0.1:1813".parse().unwrap();

        let mut listener = reactor.listen(server_addr).await.unwrap();

        let mut ctx = noop_context();
        let mut accept = listener.accept().boxed();

        match accept.poll_unpin(&mut ctx) {
            Poll::Pending => {}
            _ => {
                assert!(false, "expect pending")
            }
        }

        let mut connection = reactor.connect(server_addr, Some(client_addr)).boxed();

        match connection.poll_unpin(&mut ctx) {
            Poll::Pending => {}
            _ => {
                assert!(false, "expect pending")
            }
        }

        while reactor.wakers() != 0 {
            reactor.poll_once(Duration::from_secs(1)).unwrap();
        }

        let _server_connection = accept.await.unwrap();

        let _client_connection = connection.await.unwrap();
    }
}
