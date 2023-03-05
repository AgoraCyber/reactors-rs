use criterion::{criterion_group, criterion_main, Criterion};
use reactors::fs::{FileReactor, FileReactorEx};

use hex::ToHex;
use rand::{rngs::OsRng, RngCore};
use std::{env::temp_dir, fs::create_dir, path::PathBuf, thread::spawn, time::Duration};
use tokio::{io::AsyncWriteExt, runtime::Runtime};

fn prepare_test_dir() -> PathBuf {
    let mut dir_name = [0u8; 32];

    OsRng.fill_bytes(&mut dir_name);

    let path = temp_dir().join(dir_name.encode_hex::<String>());

    create_dir(path.clone()).unwrap();

    log::debug!("path {:?}", path);

    path
}

fn criterion_benchmark(c: &mut Criterion) {
    let dir = prepare_test_dir();

    let file_reactor = FileReactor::new();

    let loop_reactor = file_reactor.clone();

    spawn(move || loop {
        loop_reactor.poll_once(Duration::from_millis(200)).unwrap();
    });

    let mut group = c.benchmark_group("async file(c/w)");

    group.measurement_time(Duration::from_secs(10));

    group.bench_function("tokio", |b| {
        b.to_async(Runtime::new().unwrap()).iter(|| async {
            let mut file = tokio::fs::File::create(dir.join("1")).await.unwrap();
            file.write_all(&['0' as u8; 100]).await.unwrap();
        });
    });

    group.bench_function("reactor", |b| {
        use futures::AsyncWriteExt;

        b.to_async(Runtime::new().unwrap()).iter(|| async {
            let mut file = file_reactor
                .clone()
                .create_file(dir.join("1"))
                .await
                .unwrap();
            file.write_all(&['0' as u8; 100]).await.unwrap();
        });
    });

    group.finish();
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
