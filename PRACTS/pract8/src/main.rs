use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Waker};
use std::thread;
use std::time::{Duration, Instant};

struct MeasurableFuture<Fut> {
    inner_future: Fut,
    started_at: Option<Instant>,
}

impl<Fut> MeasurableFuture<Fut> {
    fn new(inner_future: Fut) -> Self {
        MeasurableFuture {
            inner_future,
            started_at: None,
        }
    }
}

impl<Fut: Future> Future for MeasurableFuture<Fut> {
    type Output = Fut::Output;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = unsafe { self.get_unchecked_mut() };

        if this.started_at.is_none() {
            this.started_at = Some(Instant::now());
        }

        let inner = unsafe { Pin::new_unchecked(&mut this.inner_future) };
        match inner.poll(cx) {
            Poll::Ready(value) => {
                let elapsed = this.started_at.unwrap().elapsed();
                println!("[measure] inner future completed in {:?}", elapsed);
                Poll::Ready(value)
            }
            Poll::Pending => Poll::Pending,
        }
    }
}

struct DelayState {
    completed: bool,
    waker: Option<Waker>,
}

struct Delay {
    deadline: Instant,
    state: Arc<Mutex<DelayState>>,
    started: bool,
}

impl Delay {
    fn new(duration: Duration) -> Self {
        Delay {
            deadline: Instant::now() + duration,
            state: Arc::new(Mutex::new(DelayState {
                completed: false,
                waker: None,
            })),
            started: false,
        }
    }
}

impl Future for Delay {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        {
            let mut guard = self.state.lock().unwrap();
            if guard.completed {
                return Poll::Ready(());
            }
            guard.waker = Some(cx.waker().clone());
        }

        if !self.started {
            self.started = true;
            let state = Arc::clone(&self.state);
            let deadline = self.deadline;
            thread::spawn(move || {
                let now = Instant::now();
                if deadline > now {
                    thread::sleep(deadline - now);
                }
                let mut guard = state.lock().unwrap();
                guard.completed = true;
                if let Some(waker) = guard.waker.take() {
                    waker.wake();
                }
            });
        }

        Poll::Pending
    }
}

async fn slow_task(label: &str, ms: u64) -> String {
    Delay::new(Duration::from_millis(ms)).await;
    format!("{label} done after {ms}ms")
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    println!("--- Delay future ---");
    let result = MeasurableFuture::new(slow_task("task A", 500)).await;
    println!("[result] {result}");

    println!();
    println!("--- two delays sequentially via measurement ---");
    let result = MeasurableFuture::new(async {
        Delay::new(Duration::from_millis(200)).await;
        Delay::new(Duration::from_millis(300)).await;
        "chained"
    })
    .await;
    println!("[result] {result}");

    println!();
    println!("--- joined delays via tokio::join ---");
    let result = MeasurableFuture::new(async {
        let (a, b, c) = tokio::join!(
            slow_task("X", 400),
            slow_task("Y", 300),
            slow_task("Z", 500),
        );
        format!("{a} | {b} | {c}")
    })
    .await;
    println!("[result] {result}");
}
