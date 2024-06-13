## broadcast-sink

![Build status](https://github.com/pragmaxim-com/broadcast-sink.rs/workflows/Rust/badge.svg)
[![Cargo](https://img.shields.io/crates/v/broadcast-sink.svg)](https://crates.io/crates/broadcast-sink)
[![Documentation](https://docs.rs/broadcast-sink/badge.svg)](https://docs.rs/broadcast-sink)

A stream adapter that broadcasts each element to consumers which execute it in parallel.
Each consumer is represented by an element-consuming task with back-pressure established through
[barrier](https://docs.rs/tokio/latest/tokio/sync/struct.Barrier.html) so that next element
is polled when last element is processed by all consumers.

## Usage

Let's implement `Consumer` interface such that each consumer mutates shared state for each element
in parallel. Note that consumers usually have some `State` or `Database` so they can write to it.

```rust
use futures::stream;
use std::sync::{Arc, RwLock};
use broadcast_sink::{Consumer, StreamBroadcastSinkExt};

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
        println!("Consumer X processed item");
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
        println!("Consumer Y processed item");
    }
}

let state = Arc::new(State {
    x: RwLock::new(1),
    y: RwLock::new(1),
});
let consumers: Vec<Arc<dyn Consumer<u64>>> = vec![
    Arc::new(MultiplyX::new(Arc::clone(&state))),
    Arc::new(MultiplyY::new(Arc::clone(&state))),
];
stream::iter(1..=5).broadcast(100, consumers).await;
assert_eq!(*state.x.read().unwrap(), 3125);
assert_eq!(*state.y.read().unwrap(), 100000);
```
