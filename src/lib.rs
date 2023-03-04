#![doc = include_str!("../README.md")]

pub mod file;
pub mod reactor;

#[cfg_attr(target_family = "unix", path = "poll_unix/mod.rs")]
pub mod poll_unix;

#[cfg(test)]
mod tests {
    use std::{
        env::temp_dir,
        io::Write,
        thread::spawn,
        time::{Duration, SystemTime},
    };

    use hex::ToHex;
    use rand::{rngs::OsRng, RngCore};
    use std::{fs::create_dir_all, path::PathBuf};

    use crate::file::{File, FileReactor};

    fn prepare_test_dir() -> PathBuf {
        let mut dir_name = [0u8; 32];

        OsRng.fill_bytes(&mut dir_name);

        let path = temp_dir().join(dir_name.encode_hex::<String>());

        create_dir_all(path.clone()).unwrap();

        log::debug!("path {:?}", path);

        path
    }

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

        for _ in 0..10000 {
            file.write_all(&['0' as u8; 1024 * 1024]).await.unwrap();
        }

        log::debug!("reactor elapsed: {:?}", now.elapsed());
    }

    #[tokio::test]
    async fn test_tokio_write() {
        use tokio::io::AsyncWriteExt;
        _ = pretty_env_logger::try_init();

        let test_dir = prepare_test_dir();

        let mut file = tokio::fs::File::create(test_dir.join("1")).await.unwrap();

        let now = SystemTime::now();

        for _ in 0..10000 {
            file.write_all(&['0' as u8; 1024 * 1024]).await.unwrap();
        }

        log::debug!("tokio elapsed: {:?}", now.elapsed());
    }

    #[test]
    fn test_std_write() {
        _ = pretty_env_logger::try_init();

        let test_dir = prepare_test_dir();

        let mut file = std::fs::File::create(test_dir.join("1")).unwrap();

        let now = SystemTime::now();

        for _ in 0..10000 {
            file.write_all(&['0' as u8; 1024 * 1024]).unwrap();
        }

        log::debug!("std elapsed: {:?}", now.elapsed());
    }
}
