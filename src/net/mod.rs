pub mod reactor;
pub mod socket;

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use async_std::task::sleep;
    use futures::{SinkExt, TryStreamExt};

    use super::{reactor::NetReactor, socket::UdpSocketReactor};

    fn create_reactor() -> NetReactor {
        let reactor = NetReactor::new();

        let loop_reactor = reactor.clone();

        std::thread::spawn(move || loop {
            loop_reactor.poll_once(Duration::from_millis(200)).unwrap();
        });

        reactor
    }

    #[async_std::test]
    async fn test_udp() {
        _ = pretty_env_logger::try_init();

        let mut reactor = create_reactor();

        let server_addr = "0.0.0.0:1812".parse().unwrap();

        let mut udp_server = reactor.udp(server_addr, 1024).await.unwrap();

        async_std::task::spawn(async move {
            while let Some((buff, remote)) = udp_server.try_next().await.unwrap() {
                log::debug!(
                    "recv data: {} from {}",
                    String::from_utf8_lossy(&buff),
                    remote
                );
            }
        });

        sleep(Duration::from_secs(1)).await;

        let mut udp_client = reactor
            .udp("0.0.0.0:0".parse().unwrap(), 1024)
            .await
            .unwrap();

        udp_client
            .send((b"hello world".to_vec(), server_addr.clone()))
            .await
            .unwrap();

        sleep(Duration::from_secs(20)).await;
    }
}
