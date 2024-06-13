use broadcast_sink::{Consumer, StreamBroadcastSinkExt};
use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use futures::stream::{self, Stream};
use std::sync::{Arc, RwLock};
use tokio::runtime::Runtime;

#[derive(Debug)]
struct State {
    x: RwLock<u64>,
    y: RwLock<u64>,
}

struct MultiplyX {
    state: Arc<State>,
}

impl MultiplyX {
    fn new(state: Arc<State>) -> Self {
        Self { state }
    }
}

impl Consumer<u64> for MultiplyX {
    fn consume(&self, _: &u64) {
        let mut x = self.state.x.write().unwrap();
        *x *= 5;
        println!("Consumer 1 processed item");
    }
}

struct MultiplyY {
    state: Arc<State>,
}

impl MultiplyY {
    fn new(state: Arc<State>) -> Self {
        Self { state }
    }
}

impl Consumer<u64> for MultiplyY {
    fn consume(&self, _: &u64) {
        let mut y = self.state.y.write().unwrap();
        *y *= 10;
        println!("Consumer 2 processed item");
    }
}

async fn batch(stream: impl Stream<Item = u64>) {
    let state = Arc::new(State {
        x: RwLock::new(1),
        y: RwLock::new(1),
    });

    let consumers: Vec<Arc<dyn Consumer<u64>>> = vec![
        Arc::new(MultiplyX::new(Arc::clone(&state))),
        Arc::new(MultiplyY::new(Arc::clone(&state))),
    ];

    let _ = stream.broadcast(100, consumers);
}

fn criterion_benchmark(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    let mut group = c.benchmark_group("broadcast_sink");
    for &size in &[10, 100, 1000, 10_000, 100_000] {
        group.throughput(Throughput::Bytes(size as u64));
        group.bench_with_input(
            BenchmarkId::from_parameter(size),
            &size,
            |bencher, &size| {
                bencher.to_async(&rt).iter(|| batch(stream::iter(0..size)));
            },
        );
    }
    group.finish();
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
