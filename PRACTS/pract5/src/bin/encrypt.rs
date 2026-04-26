use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Key, Nonce};
use clap::Parser;
use crossbeam_channel::bounded;
use rand::RngCore;
use walkdir::WalkDir;

#[derive(Parser)]
struct Args {
    #[arg(long)]
    dir: PathBuf,
}

struct FileJob {
    path: PathBuf,
    content: Vec<u8>,
}

fn main() {
    let args = Args::parse();

    let mut key_bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut key_bytes);
    let key = Key::<Aes256Gcm>::from_slice(&key_bytes).to_owned();

    let counter = Arc::new(AtomicUsize::new(0));
    let done = Arc::new(AtomicBool::new(false));
    let (tx, rx) = bounded::<FileJob>(8);

    let dir = args.dir.clone();
    let producer = thread::spawn(move || {
        for entry in WalkDir::new(&dir).into_iter().filter_map(Result::ok) {
            if !entry.file_type().is_file() {
                continue;
            }
            match fs::read(entry.path()) {
                Ok(content) => {
                    let job = FileJob {
                        path: entry.path().to_path_buf(),
                        content,
                    };
                    if tx.send(job).is_err() {
                        break;
                    }
                }
                Err(e) => eprintln!("[producer] read {}: {}", entry.path().display(), e),
            }
        }
        println!("[producer] done");
    });

    let mut workers = Vec::new();
    for id in 0..3 {
        let rx = rx.clone();
        let counter = Arc::clone(&counter);
        let key = key;
        workers.push(thread::spawn(move || {
            let cipher = Aes256Gcm::new(&key);
            while let Ok(job) = rx.recv() {
                let mut nonce_bytes = [0u8; 12];
                rand::thread_rng().fill_bytes(&mut nonce_bytes);
                let nonce = Nonce::from_slice(&nonce_bytes);

                match cipher.encrypt(nonce, job.content.as_ref()) {
                    Ok(mut encrypted) => {
                        let mut output = nonce_bytes.to_vec();
                        output.append(&mut encrypted);
                        let mut out_path = job.path.clone();
                        let new_name = format!(
                            "{}.data",
                            job.path.file_name().unwrap().to_string_lossy()
                        );
                        out_path.set_file_name(new_name);
                        if let Err(e) = fs::write(&out_path, output) {
                            eprintln!("[worker {id}] write {}: {}", out_path.display(), e);
                        } else {
                            counter.fetch_add(1, Ordering::SeqCst);
                        }
                    }
                    Err(e) => eprintln!("[worker {id}] encrypt: {e}"),
                }
            }
            println!("[worker {id}] done");
        }));
    }
    drop(rx);

    let counter_reader = {
        let counter = Arc::clone(&counter);
        let done = Arc::clone(&done);
        thread::spawn(move || {
            let mut last = 0;
            while !done.load(Ordering::SeqCst) {
                let current = counter.load(Ordering::SeqCst);
                if current != last {
                    println!("[counter] processed: {current}");
                    last = current;
                }
                thread::sleep(Duration::from_millis(50));
            }
            let final_value = counter.load(Ordering::SeqCst);
            if final_value != last {
                println!("[counter] processed: {final_value}");
            }
        })
    };

    producer.join().unwrap();
    for w in workers {
        w.join().unwrap();
    }
    done.store(true, Ordering::SeqCst);
    counter_reader.join().unwrap();

    println!("total processed: {}", counter.load(Ordering::SeqCst));
}
