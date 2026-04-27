use std::io::{self, BufRead};
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};
use clap::Parser;
use futures::stream::{self, StreamExt};
use tokio::fs;
use tokio::io::AsyncWriteExt;
use tokio::sync::Semaphore;

#[derive(Parser)]
struct Args {
    #[arg(long)]
    max_threads: Option<usize>,

    #[arg(long, default_value = "./downloads")]
    out: PathBuf,

    file: Option<PathBuf>,
}

fn decode_text(raw: &[u8]) -> String {
    if raw.starts_with(&[0xFF, 0xFE]) {
        let words: Vec<u16> = raw[2..]
            .chunks_exact(2)
            .map(|b| u16::from_le_bytes([b[0], b[1]]))
            .collect();
        String::from_utf16_lossy(&words)
    } else if raw.starts_with(&[0xFE, 0xFF]) {
        let words: Vec<u16> = raw[2..]
            .chunks_exact(2)
            .map(|b| u16::from_be_bytes([b[0], b[1]]))
            .collect();
        String::from_utf16_lossy(&words)
    } else if raw.starts_with(&[0xEF, 0xBB, 0xBF]) {
        String::from_utf8_lossy(&raw[3..]).to_string()
    } else {
        String::from_utf8_lossy(raw).to_string()
    }
}

fn read_lines(args: &Args) -> Result<Vec<String>> {
    if let Some(path) = &args.file {
        let raw = std::fs::read(path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        let content = decode_text(&raw);
        Ok(content
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .map(String::from)
            .collect())
    } else {
        let stdin = io::stdin();
        let mut lines = Vec::new();
        for line in stdin.lock().lines() {
            let line = line.context("stdin read error")?;
            let trimmed = line.trim();
            if !trimmed.is_empty() {
                lines.push(trimmed.to_string());
            }
        }
        Ok(lines)
    }
}

fn name_from_url(url: &str, index: usize) -> String {
    let raw = url.split('?').next().unwrap_or(url);
    let last = raw.rsplit('/').next().unwrap_or("");
    let stem = last
        .trim_end_matches('/')
        .split('#')
        .next()
        .unwrap_or("");
    if stem.is_empty() {
        format!("{index}_index.html")
    } else if stem.contains('.') {
        format!("{index}_{stem}")
    } else {
        format!("{index}_{stem}.html")
    }
}

async fn download(
    client: reqwest::Client,
    url: String,
    out: PathBuf,
    index: usize,
) -> Result<()> {
    let resp = client
        .get(&url)
        .send()
        .await
        .with_context(|| format!("request failed: {url}"))?;
    let status = resp.status();
    let body = resp.bytes().await.context("body read failed")?;

    let name = name_from_url(&url, index);
    let path = out.join(&name);
    let mut file = fs::File::create(&path)
        .await
        .with_context(|| format!("create {}", path.display()))?;
    file.write_all(&body).await.context("write failed")?;
    file.flush().await.ok();

    println!("[{status}] {url} -> {}", path.display());
    Ok(())
}

fn main() -> Result<()> {
    let args = Args::parse();
    let max_threads = args.max_threads.unwrap_or_else(|| {
        std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(4)
    });

    let urls = read_lines(&args)?;
    if urls.is_empty() {
        anyhow::bail!("no URLs provided");
    }

    std::fs::create_dir_all(&args.out)
        .with_context(|| format!("create {}", args.out.display()))?;

    println!(
        "[config] runtime threads: {}, urls: {}, out: {}",
        max_threads,
        urls.len(),
        args.out.display()
    );

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(max_threads)
        .enable_all()
        .build()
        .context("failed to build tokio runtime")?;

    runtime.block_on(async {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .context("failed to build http client")?;

        let semaphore = Arc::new(Semaphore::new(max_threads * 2));

        let tasks = stream::iter(urls.into_iter().enumerate().map(|(i, url)| {
            let client = client.clone();
            let out = args.out.clone();
            let sem = Arc::clone(&semaphore);
            async move {
                let _permit = sem.acquire().await.expect("semaphore closed");
                if let Err(e) = download(client, url.clone(), out, i).await {
                    eprintln!("[error] {url}: {e:#}");
                }
            }
        }))
        .buffer_unordered(max_threads * 2);

        tasks.for_each(|()| async {}).await;
        Ok::<(), anyhow::Error>(())
    })?;

    Ok(())
}
