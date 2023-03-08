use std::{io::ErrorKind, task::Poll, time::Duration};

use futures::{FutureExt, SinkExt, TryStreamExt};
use futures_test::task::noop_context;
use reactors::{
    io::{IoReactor, UdpSocket},
    Reactor,
};

#[futures_test::test]
async fn udp_test() {
    _ = pretty_env_logger::try_init();

    let reactor = IoReactor::default();

    let server_addr = "127.0.0.1:1812".parse().unwrap();

    let client_addr = "127.0.0.1:1813".parse().unwrap();

    let server = UdpSocket::new(reactor.clone(), server_addr).unwrap();

    let client = UdpSocket::new(reactor.clone(), client_addr).unwrap();

    let mut client_send = client.to_write_stream(None);

    let mut server_receiver = server.to_read_stream(1024, None);

    let mut send = client_send.send((b"hello world".to_vec(), server_addr));

    while send.poll_unpin(&mut noop_context()).is_pending() {}

    let mut recv = server_receiver.try_next();

    loop {
        match recv.poll_unpin(&mut noop_context()) {
            Poll::Pending => break,
            Poll::Ready(r) => {
                let (data, remote) = r.unwrap().unwrap();

                assert_eq!(data, b"hello world");

                assert_eq!(remote, client_addr);
            }
        }
    }
}

#[futures_test::test]
async fn udp_timeout() {
    _ = pretty_env_logger::try_init();

    let mut reactor = IoReactor::default();

    let server_addr = "127.0.0.1:1812".parse().unwrap();

    let server = UdpSocket::new(reactor.clone(), server_addr).unwrap();

    let mut read = server.to_read_stream(1024, Duration::from_secs(1));

    let mut try_next = read.try_next();

    assert!(try_next.poll_unpin(&mut noop_context()).is_pending());

    reactor.poll_once(Duration::from_secs(2)).unwrap();

    let result = try_next.await;

    assert_eq!(result.unwrap_err().kind(), ErrorKind::TimedOut);
}
