# reactors

[!["Crates.io version"](https://img.shields.io/crates/v/reactors.svg)](https://crates.io/crates/reactors) [!["docs.rs docs"](https://img.shields.io/badge/docs-latest-blue.svg)](https://docs.rs/reactors)

**reactors** is a low-level cross-platform asynchronous io wrapper library for rust:

* `linux/android` epoll backend
* `macos/ios/freebsd` kqueue backend
* `windows` iocp backend

[`futures`](https://docs.rs/futures/0.3.27/futures/)

## bench

Perhaps **reactors** is much faster than the [`tokio`](https://docs.rs/tokio/1.26.0/tokio/) library 😂 , for the detailed code of the bench test please visit [`echo bench`](benches/echo.rs)

![`bench`](./bench.png)

## Thread Model

**reactors** does not limit the thread model it uses. In particular, we can create multiple IoReactor objects to share the IO load

```rust
use crate::io::*;
use crate::io::socket::tcp::*;
use std::thread::spawn;
use futures::TryStreamExt;
use futures::{AsyncReadExt, AsyncWriteExt};
use futures::executor::block_on;

fn main() {
    let acceptor_reactor = IoReactor::default();
    let connection_reactor = IoReactor::default();

    let acceptor_reactor_background = acceptor_reactor.clone();
    let connection_reactor_background = connection_reactor.clone();

    spawn(move || {
        acceptor_reactor_background.poll_once(Duration::from_millsec(100)).unwrap();
    });

    spawn(move || {
        connection_reactor_background.poll_once(Duration::from_millsec(100)).unwrap();
    });

    let mut acceptor = TcpAcceptor::new(acceptor_reactor, "127.0.0.1:1813".parse()?, Some(connection_reactor))?;

    block_on(async {
        while let Some((conn, _)) = acceptor.try_next().await? {
            let mut reader = conn.to_read_stream(None);
            let mut writer = conn.to_write_stream(None);

            let mut buff = [0u8; 11];

            reader.read_exact(&mut buff).await?;

            assert_eq!(&buff, b"hello world");

            writer.write_all(&buff).await?;
        }
    });

}
```
