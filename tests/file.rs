use std::{io::SeekFrom, path::PathBuf, task::Poll, time::Duration};

use futures::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt, FutureExt};
use futures_test::task::noop_context;
use reactors::{
    io::{file::*, IoReactor},
    Reactor,
};

#[test]
fn test_file_rw() {
    _ = pretty_env_logger::try_init();

    let mut reactor = IoReactor::default();

    let dir: PathBuf = env!("CARGO_TARGET_TMPDIR").into();

    let file = File::create(reactor.clone(), dir.join("test")).unwrap();

    let mut write_stream = file.to_write_stream(None);

    let mut read_stream = file.to_read_stream(None);

    let mut write_all = write_stream.write_all("hello world".as_bytes());

    loop {
        match write_all.poll_unpin(&mut noop_context()) {
            Poll::Pending => {
                reactor.poll_once(Duration::from_secs(1)).unwrap();
            }
            Poll::Ready(result) => {
                result.unwrap();
                break;
            }
        }
    }

    match read_stream
        .seek(SeekFrom::Start(0))
        .poll_unpin(&mut noop_context())
    {
        Poll::Pending => panic!("unexpect pending"),
        Poll::Ready(result) => {
            assert_eq!(result.unwrap(), 0);
        }
    }

    let mut buff = [0u8; 100];

    let mut read = read_stream.read(&mut buff);

    loop {
        match read.poll_unpin(&mut noop_context()) {
            Poll::Pending => {
                reactor.poll_once(Duration::from_secs(1)).unwrap();
            }
            Poll::Ready(result) => {
                assert_eq!(result.unwrap(), 11);
                assert_eq!(&buff[..11], "hello world".as_bytes());
                break;
            }
        }
    }
}
