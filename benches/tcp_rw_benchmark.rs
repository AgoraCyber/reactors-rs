use std::time::Duration;

use criterion::{criterion_group, criterion_main, Criterion};

use tokio::runtime::Runtime;

fn criterion_benchmark(c: &mut Criterion) {
    let mut group = c.benchmark_group("async file(c/w)");

    group.measurement_time(Duration::from_secs(10));

    // group.bench_function("tokio", |b| {
    //     b.to_async(Runtime::new().unwrap()).iter(|| async {
    //         let mut file = tokio::fs::File::create(dir.join("1")).await.unwrap();
    //         file.write_all(&['0' as u8; 100]).await.unwrap();
    //     });
    // });

    group.bench_function("reactor", |b| {
        b.to_async(Runtime::new().unwrap()).iter(|| async {});
    });

    group.finish();
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
