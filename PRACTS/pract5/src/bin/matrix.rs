use std::thread;

use crossbeam_channel::bounded;
use rand::Rng;
use rayon::prelude::*;

const SIZE: usize = 4096;
const MATRICES_PER_CONSUMER: usize = 2;

type Matrix = Vec<Vec<u32>>;

fn generate_matrix() -> Matrix {
    let mut rng = rand::thread_rng();
    (0..SIZE)
        .map(|_| (0..SIZE).map(|_| rng.gen_range(0..100)).collect())
        .collect()
}

fn parallel_sum(matrix: &Matrix) -> u64 {
    matrix
        .par_iter()
        .map(|row| row.iter().map(|&x| u64::from(x)).sum::<u64>())
        .sum()
}

fn main() {
    let total_matrices = MATRICES_PER_CONSUMER * 2;
    let (tx, rx) = bounded::<Matrix>(2);

    let producer = thread::spawn(move || {
        for i in 0..total_matrices {
            println!("[producer] generating matrix {}", i + 1);
            let m = generate_matrix();
            tx.send(m).expect("send failed");
        }
        println!("[producer] done");
    });

    let mut consumers = Vec::new();
    for id in 0..2 {
        let rx = rx.clone();
        consumers.push(thread::spawn(move || {
            while let Ok(matrix) = rx.recv() {
                let sum = parallel_sum(&matrix);
                println!("[consumer {id}] sum = {sum}");
            }
            println!("[consumer {id}] done");
        }));
    }
    drop(rx);

    producer.join().unwrap();
    for c in consumers {
        c.join().unwrap();
    }
}
