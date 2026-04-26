use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Instant;

use lab6::my_arc::MyArc;
use lab6::my_mutex::MyMutex;

const THREADS: usize = 8;
const ITERS_PER_THREAD: usize = 200_000;
const ARC_CLONES_PER_THREAD: usize = 1_000_000;

fn bench_std_mutex() -> u128 {
    let counter = Arc::new(Mutex::new(0u64));
    let start = Instant::now();
    let mut handles = Vec::new();
    for _ in 0..THREADS {
        let counter = Arc::clone(&counter);
        handles.push(thread::spawn(move || {
            for _ in 0..ITERS_PER_THREAD {
                let mut g = counter.lock().unwrap();
                *g += 1;
            }
        }));
    }
    for h in handles {
        h.join().unwrap();
    }
    let elapsed = start.elapsed().as_micros();
    assert_eq!(*counter.lock().unwrap(), (THREADS * ITERS_PER_THREAD) as u64);
    elapsed
}

fn bench_my_mutex() -> u128 {
    let counter = MyArc::new(MyMutex::new(0u64));
    let start = Instant::now();
    let mut handles = Vec::new();
    for _ in 0..THREADS {
        let counter = counter.clone();
        handles.push(thread::spawn(move || {
            for _ in 0..ITERS_PER_THREAD {
                let mut g = counter.lock();
                *g += 1;
            }
        }));
    }
    for h in handles {
        h.join().unwrap();
    }
    let elapsed = start.elapsed().as_micros();
    assert_eq!(*counter.lock(), (THREADS * ITERS_PER_THREAD) as u64);
    elapsed
}

fn bench_std_arc_clone() -> u128 {
    let arc = Arc::new(42u64);
    let start = Instant::now();
    let mut handles = Vec::new();
    for _ in 0..THREADS {
        let arc = Arc::clone(&arc);
        handles.push(thread::spawn(move || {
            for _ in 0..ARC_CLONES_PER_THREAD {
                let _c = Arc::clone(&arc);
            }
        }));
    }
    for h in handles {
        h.join().unwrap();
    }
    start.elapsed().as_micros()
}

fn bench_my_arc_clone() -> u128 {
    let arc = MyArc::new(42u64);
    let start = Instant::now();
    let mut handles = Vec::new();
    for _ in 0..THREADS {
        let arc = arc.clone();
        handles.push(thread::spawn(move || {
            for _ in 0..ARC_CLONES_PER_THREAD {
                let _c = arc.clone();
            }
        }));
    }
    for h in handles {
        h.join().unwrap();
    }
    start.elapsed().as_micros()
}

fn fmt_diff(custom: u128, std: u128) -> String {
    let diff = custom as f64 / std as f64;
    let pct = (diff - 1.0) * 100.0;
    if pct >= 0.0 {
        format!("custom slower by {pct:.1}%")
    } else {
        format!("custom faster by {:.1}%", -pct)
    }
}

fn main() {
    println!("threads: {THREADS}");
    println!("mutex iters per thread: {ITERS_PER_THREAD}");
    println!("arc clones per thread: {ARC_CLONES_PER_THREAD}");
    println!();

    let std_mutex_time = bench_std_mutex();
    let my_mutex_time = bench_my_mutex();
    println!("Mutex");
    println!("  std::Mutex   : {std_mutex_time} us");
    println!("  MyMutex      : {my_mutex_time} us");
    println!("  diff         : {}", fmt_diff(my_mutex_time, std_mutex_time));
    println!();

    let std_arc_time = bench_std_arc_clone();
    let my_arc_time = bench_my_arc_clone();
    println!("Arc clone");
    println!("  std::Arc     : {std_arc_time} us");
    println!("  MyArc        : {my_arc_time} us");
    println!("  diff         : {}", fmt_diff(my_arc_time, std_arc_time));
}
