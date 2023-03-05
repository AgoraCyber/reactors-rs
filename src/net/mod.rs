pub mod reactor;
pub mod socket;

#[cfg(test)]
mod tests {

    use std::time::Duration;

    use futures::{SinkExt, StreamExt};
    use futures_test::{assert_stream_next, assert_stream_pending};

    use super::{reactor::NetReactor, socket::UdpSocketReactor};

    #[async_std::test]
    async fn test_udp() {
        _ = pretty_env_logger::try_init();

        let mut reactor = NetReactor::new();

        let server_addr = "127.0.0.1:1812".parse().unwrap();

        let client_addr = "127.0.0.1:1813".parse().unwrap();

        let mut udp_server = reactor.udp(server_addr, 1024).await.unwrap();

        let mut udp_client = reactor.udp(client_addr, 1024).await.unwrap();

        assert_stream_pending!(udp_server);

        let mut udp_srever = udp_server.map(|c| c.unwrap());

        for i in 0..100 {
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
}
