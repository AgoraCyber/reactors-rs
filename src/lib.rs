#![doc = include_str!("../README.md")]

pub mod fs;
pub mod net;
pub mod reactor;

#[cfg_attr(target_family = "unix", path = "poller/mod.rs")]
pub mod poller;

#[cfg(test)]
mod tests {
    use std::{
        env::temp_dir,
        io::Write,
        thread::spawn,
        time::{Duration, SystemTime},
    };

    use async_std::io::ReadExt;
    use futures::AsyncSeekExt;
    use hex::ToHex;
    use rand::{rngs::OsRng, RngCore};
    use std::{fs::create_dir_all, path::PathBuf};

    use super::fs::{FileEx, FileReactor};

    fn prepare_test_dir() -> PathBuf {
        let mut dir_name = [0u8; 32];

        OsRng.fill_bytes(&mut dir_name);

        let path = temp_dir().join(dir_name.encode_hex::<String>());

        create_dir_all(path.clone()).unwrap();

        log::debug!("path {:?}", path);

        path
    }

    static LOOP: usize = 100;

    #[tokio::test]
    async fn test_reactor_write() {
        use futures::AsyncWriteExt;

        _ = pretty_env_logger::try_init();

        let test_dir = prepare_test_dir();

        let mut file_reactor = FileReactor::new();

        let loop_reactor = file_reactor.clone();

        spawn(move || loop {
            loop_reactor.poll_once(Duration::from_millis(200)).unwrap();
        });

        let mut file = file_reactor.create_file(test_dir.join("1")).await.unwrap();
        let now = SystemTime::now();

        for _ in 0..LOOP {
            file.write_all(&['0' as u8; 1024 * 1024]).await.unwrap();
        }

        log::debug!("reactor elapsed: {:?}", now.elapsed());
    }

    #[tokio::test]
    async fn test_reactor() {
        use futures::AsyncWriteExt;

        _ = pretty_env_logger::try_init();

        let test_dir = prepare_test_dir();

        let mut file_reactor = FileReactor::new();

        let loop_reactor = file_reactor.clone();

        spawn(move || loop {
            loop_reactor.poll_once(Duration::from_millis(200)).unwrap();
        });

        let mut file = file_reactor.create_file(test_dir.join("1")).await.unwrap();

        file.write_all(&['1' as u8; 1024]).await.unwrap();

        file.write_all(&['2' as u8; 1024]).await.unwrap();

        file.seek(std::io::SeekFrom::Start(0)).await.unwrap();

        let mut buff = vec![];

        file.read_to_end(&mut buff).await.unwrap();

        assert_eq!(&buff[0..1024], ['1' as u8; 1024]);
        assert_eq!(&buff[1024..], ['2' as u8; 1024]);
    }

    #[tokio::test]
    async fn test_tokio_write() {
        use tokio::io::AsyncWriteExt;
        _ = pretty_env_logger::try_init();

        let test_dir = prepare_test_dir();

        let mut file = tokio::fs::File::create(test_dir.join("1")).await.unwrap();

        let now = SystemTime::now();

        for _ in 0..LOOP {
            file.write_all(&['0' as u8; 1024 * 1024]).await.unwrap();
        }

        log::info!("tokio elapsed: {:?}", now.elapsed());
    }

    #[test]
    fn test_std_write() {
        _ = pretty_env_logger::try_init();

        let test_dir = prepare_test_dir();

        let mut file = std::fs::File::create(test_dir.join("1")).unwrap();

        let now = SystemTime::now();

        for _ in 0..LOOP {
            file.write_all(&['0' as u8; 1024 * 1024]).unwrap();
        }

        log::info!("std elapsed: {:?}", now.elapsed());
    }
}
