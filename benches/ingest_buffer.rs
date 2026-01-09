//! Benchmark for MetricsBuffer performance

use chrono::Utc;
use criterion::{black_box, criterion_group, criterion_main, Criterion, Throughput};
use query_vault::buffer::MetricsBuffer;
use query_vault::models::{QueryMetric, QueryStatus};
use uuid::Uuid;

fn create_metric() -> QueryMetric {
    QueryMetric::new(
        Uuid::new_v4(),
        Uuid::new_v4(),
        "SELECT id, name, email FROM users WHERE status = 'active' ORDER BY created_at DESC LIMIT 100".to_string(),
        QueryStatus::Success,
        42,
        Utc::now(),
    )
}

fn bench_push(c: &mut Criterion) {
    let mut group = c.benchmark_group("buffer_push");
    group.throughput(Throughput::Elements(1000));

    group.bench_function("push_1000_metrics", |b| {
        b.iter(|| {
            let buffer = MetricsBuffer::new(100_000);
            for _ in 0..1000 {
                let _ = buffer.try_push(black_box(create_metric()));
            }
        });
    });

    group.finish();
}

fn bench_pop_batch(c: &mut Criterion) {
    let mut group = c.benchmark_group("buffer_pop");
    group.throughput(Throughput::Elements(1000));

    group.bench_function("pop_batch_1000", |b| {
        b.iter_batched(
            || {
                let buffer = MetricsBuffer::new(100_000);
                for _ in 0..1000 {
                    buffer.try_push(create_metric()).unwrap();
                }
                buffer
            },
            |buffer| {
                black_box(buffer.pop_batch(1000));
            },
            criterion::BatchSize::SmallInput,
        );
    });

    group.finish();
}

fn bench_concurrent_push(c: &mut Criterion) {
    let mut group = c.benchmark_group("buffer_concurrent");
    group.throughput(Throughput::Elements(10000));

    group.bench_function("concurrent_push_10000", |b| {
        b.iter(|| {
            let buffer = MetricsBuffer::new(100_000);
            let handles: Vec<_> = (0..10)
                .map(|_| {
                    let buf = buffer.clone();
                    std::thread::spawn(move || {
                        for _ in 0..1000 {
                            let _ = buf.try_push(create_metric());
                        }
                    })
                })
                .collect();

            for h in handles {
                h.join().unwrap();
            }
        });
    });

    group.finish();
}

criterion_group!(benches, bench_push, bench_pop_batch, bench_concurrent_push);
criterion_main!(benches);
