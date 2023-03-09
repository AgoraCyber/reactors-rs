use std::{io::ErrorKind, time::Duration};

use futures::{AsyncReadExt, AsyncWriteExt, FutureExt, TryStreamExt};
use futures_test::{assert_stream_pending, task::noop_context};
use reactors::{
    io::{IoReactor, TcpAcceptor, TcpConnection},
    Reactor,
};

#[futures_test::test]
async fn test_tcp() {
    _ = pretty_env_logger::try_init();

    let mut reactor = IoReactor::default();

    let listen_addr = "127.0.0.1:1812".parse().unwrap();

    let mut acceptor = TcpAcceptor::new(reactor.clone(), listen_addr).unwrap();

    assert_stream_pending!(acceptor);

    let mut connect = TcpConnection::connect(reactor.clone(), listen_addr, None);

    assert!(connect.poll_unpin(&mut noop_context()).is_pending());

    // poll system io events.
    while reactor.poll_once(Duration::from_secs(1)).unwrap() != 0 {}

    let client_conn = connect.await.unwrap();
    let (server_conn, _) = acceptor.try_next().await.unwrap().unwrap();

    let mut server_read = server_conn.to_read_stream(Duration::from_secs(1));

    let mut buff = [0u8; 32];

    let mut read = server_read.read(&mut buff);

    assert!(read.poll_unpin(&mut noop_context()).is_pending());

    // Trigger the timeout events.
    while reactor.poll_once(Duration::from_secs(2)).unwrap() != 0 {}

    // expect timeout error
    assert_eq!(read.await.unwrap_err().kind(), ErrorKind::TimedOut);

    let mut write_stream = client_conn.to_write_stream(Duration::from_secs(1));

    let write = write_stream.write(&b"hello world"[..]);

    let read_exact = server_read.read(&mut buff);

    while reactor.poll_once(Duration::from_secs(1)).unwrap() != 0 {}

    write.await.unwrap();

    while reactor.poll_once(Duration::from_secs(1)).unwrap() != 0 {}

    assert_eq!(read_exact.await.unwrap(), 11);

    assert_eq!(&buff[..11], b"hello world");
}

#[futures_test::test]
async fn test_iocp() {
    _ = pretty_env_logger::try_init();

    let reactor = IoReactor::default();

    let listen_addr = "127.0.0.1:1812".parse().unwrap();

    let mut acceptor = TcpAcceptor::new(reactor.clone(), listen_addr).unwrap();

    assert_stream_pending!(acceptor);

    let _connect = TcpConnection::connect(reactor.clone(), listen_addr, None);
}
