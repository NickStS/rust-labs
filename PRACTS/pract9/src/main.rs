use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use clap::Parser;
use futures::stream::{StreamExt, TryStreamExt};
use tokio::fs;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::Semaphore;

#[derive(Parser)]
struct Args {
    #[arg(long)]
    max_threads: Option<usize>,

    #[arg(long, default_value = "./downloads")]
    out: PathBuf,

    #[arg(long, default_value_t = 16)]
    concurrency: usize,

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

async fn read_lines(args: &Args) -> Result<Vec<String>> {
    if let Some(path) = &args.file {
        let raw = fs::read(path)
            .await
            .with_context(|| format!("failed to open {}", path.display()))?;
        let content = decode_text(&raw);
        return Ok(content
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .map(String::from)
            .collect());
    }

    let mut lines = Vec::new();
    let stdin = tokio::io::stdin();
    let mut reader = BufReader::new(stdin).lines();
    while let Some(line) = reader.next_line().await? {
        let trimmed = line.trim().to_string();
        if !trimmed.is_empty() {
            lines.push(trimmed);
        }
    }
    Ok(lines)
}

fn name_from_url(url: &str, index: usize) -> String {
    let raw = url.split('?').next().unwrap_or(url);
    let last = raw.rsplit('/').next().unwrap_or("");
    let stem = last.trim_end_matches('/').split('#').next().unwrap_or("");
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
    sem: Arc<Semaphore>,
    url: String,
    out: PathBuf,
    index: usize,
) -> Result<()> {
    let _permit = sem.acquire_owned().await.context("semaphore closed")?;

    let resp = client
        .get(&url)
        .send()
        .await
        .with_context(|| format!("request failed: {url}"))?;
    let status = resp.status();

    let path = out.join(name_from_url(&url, index));
    let mut file = fs::File::create(&path)
        .await
        .with_context(|| format!("create {}", path.display()))?;

    let mut stream = resp.bytes_stream();
    while let Some(chunk) = stream.try_next().await.context("stream chunk")? {
        file.write_all(&chunk).await.context("write chunk")?;
    }
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

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(max_threads)
        .enable_all()
        .build()
        .context("failed to build tokio runtime")?;

    runtime.block_on(async move {
        let urls = read_lines(&args).await?;
        if urls.is_empty() {
            anyhow::bail!("no URLs provided");
        }

        fs::create_dir_all(&args.out)
            .await
            .with_context(|| format!("create {}", args.out.display()))?;

        println!(
            "[config] runtime threads: {}, concurrency: {}, urls: {}, out: {}",
            max_threads,
            args.concurrency,
            urls.len(),
            args.out.display()
        );

        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .pool_max_idle_per_host(args.concurrency)
            .build()
            .context("failed to build http client")?;

        let semaphore = Arc::new(Semaphore::new(args.concurrency));

        futures::stream::iter(urls.into_iter().enumerate())
            .for_each_concurrent(args.concurrency, |(i, url)| {
                let client = client.clone();
                let out = args.out.clone();
                let sem = Arc::clone(&semaphore);
                async move {
                    if let Err(e) = download(client, sem, url.clone(), out, i).await {
                        eprintln!("[error] {url}: {e:#}");
                    }
                }
            })
            .await;

        Ok::<(), anyhow::Error>(())
    })?;

    Ok(())
}
