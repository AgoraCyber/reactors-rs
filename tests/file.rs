use std::{io::SeekFrom, path::PathBuf};

use reactors::io::{file::File, poller::Poller};

use futures::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};

#[futures_test::test]
async fn test_file() {
    let poller = Poller::default();

    let dir: PathBuf = env!("CARGO_TARGET_TMPDIR").into();

    let file = File::create(poller.clone(), dir.join("test")).unwrap();

    let mut write_stream = file.to_write_stream(None);

    write_stream
        .write_all("hello world".as_bytes())
        .await
        .unwrap();

    let mut read_stream = file.to_read_stream(None);

    read_stream.seek(SeekFrom::Start(0)).await.unwrap();

    let mut buff = vec![];

    let len = read_stream.read_to_end(&mut buff).await.unwrap();

    assert_eq!(&buff[..len], "hello world".as_bytes());
}
