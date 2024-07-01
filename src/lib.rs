#[cfg(test)]
#[macro_use]
extern crate doc_comment;

#[cfg(test)]
doctest!("../README.md");

mod logger;

use core::future::Future;
use core::marker::PhantomPinned;
use core::pin::Pin;
use core::sync::atomic::{AtomicUsize, Ordering};
use core::task::{Context, Poll};
use futures::ready;
use futures::{Stream, StreamExt};
use pin_project_lite::pin_project;
use std::sync::Arc;
use tokio::sync::{broadcast, Barrier, Mutex};
use tokio::task;
use tokio_stream::wrappers::BroadcastStream;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BroadcastSinkError {
    message: String,
}

impl BroadcastSinkError {
    pub fn new(message: &str) -> Self {
        BroadcastSinkError {
            message: message.to_string(),
        }
    }
}

pub trait Consumer<T>: Send + Sync {
    fn consume(&mut self, item: &T) -> Result<(), BroadcastSinkError>;
}

pin_project! {
    #[must_use = "futures do nothing unless you `.await` or poll them"]
    pub struct BroadcastSink<St, T>
    where
    St: Stream<Item = T>,
    {
        #[pin]
        stream: St,
        #[pin]
        tx: broadcast::Sender<Arc<T>>,
        active_count: Arc<AtomicUsize>,
        consumers: Vec<Arc<Mutex<dyn Consumer<T>>>>,
        #[pin]
        _pin: PhantomPinned,
    }
}

impl<St, T> BroadcastSink<St, T>
where
    St: Stream<Item = T>,
    T: Send + Sync + 'static,
{
    fn new(stream: St, capacity: usize, consumers: Vec<Arc<Mutex<dyn Consumer<T>>>>) -> Self {
        let (tx, _rx) = broadcast::channel::<Arc<St::Item>>(capacity);
        let consumer_count = consumers.len();
        let barrier = Arc::new(Barrier::new(consumer_count));
        let active_count = Arc::new(AtomicUsize::new(0));
        let cs = consumers.clone();

        for consumer in consumers.into_iter() {
            let barrier_clone = Arc::clone(&barrier);
            let rx = tx.subscribe();
            let active_count_clone = Arc::clone(&active_count);

            task::spawn(async move {
                let mut stream = BroadcastStream::new(rx);
                while let Some(Ok(item)) = stream.next().await {
                    let mut consumer = consumer.lock().await;
                    if let Err(e) = consumer.consume(&item) {
                        error!("BroadcastSink consumer error occurred: {:?}", e.message);
                    }
                    barrier_clone.wait().await;
                    active_count_clone.fetch_sub(1, Ordering::SeqCst);
                }
            });
        }
        Self {
            stream,
            tx,
            active_count,
            consumers: cs,
            _pin: PhantomPinned,
        }
    }
}

impl<St, T> Future for BroadcastSink<St, T>
where
    St: Stream<Item = T>,
    T: Clone + Send + Sync + 'static,
{
    type Output = Vec<Arc<Mutex<dyn Consumer<T>>>>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut me = self.project();

        loop {
            match ready!(me.stream.as_mut().poll_next(cx)) {
                Some(item) => {
                    let next_arc = Arc::new(item);
                    me.active_count
                        .fetch_add(me.consumers.len(), Ordering::SeqCst);
                    let _ = me.tx.send(next_arc); // TODO handle error
                }
                None => {
                    let active_count = me.active_count.load(Ordering::SeqCst);
                    if active_count == 0 {
                        return Poll::Ready(me.consumers.to_vec());
                    }
                }
            };
        }
    }
}

pub trait StreamBroadcastSinkExt: Stream {
    fn broadcast(
        self,
        capacity: usize,
        consumers: Vec<Arc<Mutex<dyn Consumer<Self::Item>>>>,
    ) -> BroadcastSink<Self, Self::Item>
    where
        Self: Sized,
        <Self as Stream>::Item: Sync + Send + 'static,
    {
        BroadcastSink::new(self, capacity, consumers)
    }
}

impl<T: ?Sized> StreamBroadcastSinkExt for T where T: Stream {}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::stream;
    use std::sync::RwLock;

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
        fn consume(&mut self, _: &u64) -> Result<(), BroadcastSinkError> {
            let mut x = self.state.x.write().unwrap();
            *x *= 5;
            println!("Consumer X processed item");
            Ok(())
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
        fn consume(&mut self, _: &u64) -> Result<(), BroadcastSinkError> {
            let mut y = self.state.y.write().unwrap();
            *y *= 10;
            println!("Consumer Y processed item");
            Ok(())
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn test_stream_broadcast_ext() {
        let state = Arc::new(State {
            x: RwLock::new(1),
            y: RwLock::new(1),
        });

        let consumers = stream::iter(1..=5)
            .broadcast(
                100,
                vec![
                    Arc::new(Mutex::new(MultiplyX::new(Arc::clone(&state)))),
                    Arc::new(Mutex::new(MultiplyY::new(Arc::clone(&state)))),
                ],
            )
            .await;

        assert_eq!(*state.x.read().unwrap(), 3125);
        assert_eq!(*state.y.read().unwrap(), 100000);

        stream::iter(1..=5).broadcast(100, consumers).await;

        assert_eq!(*state.x.read().unwrap(), 9765625);
        assert_eq!(*state.y.read().unwrap(), 10000000000);
    }
}
