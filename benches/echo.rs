use std::{thread::spawn, time::Duration};

use criterion::{async_executor::FuturesExecutor, *};
use futures::executor::block_on;
use reactors::Reactor;

async fn setup_tokio_server() -> anyhow::Result<()> {
    use tokio::io::AsyncWriteExt;
    use tokio::{io::AsyncReadExt, net::TcpListener};

    let listener = TcpListener::bind("127.0.0.1:1812").await?;

    loop {
        let (mut conn, _) = listener.accept().await?;

        let mut buff = [0u8; 11];

        conn.read_exact(&mut buff).await?;

        assert_eq!(&buff, b"hello world");

        conn.write_all(&buff).await?;
    }
}

async fn tokio_client() -> anyhow::Result<()> {
    use tokio::io::AsyncWriteExt;
    use tokio::{io::AsyncReadExt, net::TcpStream};

    let mut conn = TcpStream::connect("127.0.0.1:1812").await?;

    let mut buff = [0u8; 11];

    conn.write_all(&b"hello world"[..]).await?;

    conn.read_exact(&mut buff).await?;

    assert_eq!(&buff, b"hello world");

    Ok(())
}

fn bench_tokio(c: &mut Criterion) {
    spawn(|| {
        use tokio::runtime::Runtime;

        let rt = Runtime::new().unwrap();

        rt.block_on(setup_tokio_server()).unwrap();
    });

    let rt = tokio::runtime::Runtime::new().unwrap();

    c.bench_function("echo tokio", |b| b.to_async(&rt).iter(|| tokio_client()));
}

async fn setup_reactors_server(reactor: reactors::io::IoReactor) -> anyhow::Result<()> {
    use futures::TryStreamExt;
    use futures::{AsyncReadExt, AsyncWriteExt};
    use reactors::io::socket::tcp::TcpAcceptor;

    let mut acceptor = TcpAcceptor::new(reactor, "127.0.0.1:1813".parse()?, None)?;

    while let Some((conn, remote)) = acceptor.try_next().await? {
        log::debug!("accept {}", remote);
        let mut reader = conn.to_read_stream(None);
        let mut writer = conn.to_write_stream(None);

        let mut buff = [0u8; 11];

        reader.read_exact(&mut buff).await?;

        assert_eq!(&buff, b"hello world");

        writer.write_all(&buff).await?;
    }

    Ok(())
}

async fn reactor_client(reactor: reactors::io::IoReactor) -> anyhow::Result<()> {
    use futures::{AsyncReadExt, AsyncWriteExt};
    use reactors::io::socket::tcp::TcpStream;

    let conn = TcpStream::connect(reactor, "127.0.0.1:1813".parse()?, None, None).await?;

    let mut reader = conn.to_read_stream(None);
    let mut writer = conn.to_write_stream(None);

    let mut buff = [0u8; 11];

    writer.write_all(&b"hello world"[..]).await?;

    reader.read_exact(&mut buff).await?;

    assert_eq!(&buff, b"hello world");

    Ok(())
}

fn bench_reactors(c: &mut Criterion) {
    pretty_env_logger::init();
    use reactors::io::IoReactor;

    let reactor = IoReactor::default();

    let mut server_background_reactor = reactor.clone();

    spawn(move || loop {
        server_background_reactor
            .poll_once(Duration::from_millis(10000))
            .unwrap();
    });

    let server_reactor = reactor.clone();

    spawn(move || {
        block_on(setup_reactors_server(server_reactor)).unwrap();
    });

    c.bench_function("echo reactors", |b| {
        b.to_async(FuturesExecutor)
            .iter(|| reactor_client(reactor.clone()))
    });
}

criterion_group!(benches, bench_reactors, bench_tokio);
criterion_main!(benches);
