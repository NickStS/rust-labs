//! Async image editor with CPU-bound work offloaded to a blocking thread pool.

#![warn(
    missing_docs,
    missing_crate_level_docs,
    clippy::missing_panics_doc,
    clippy::missing_errors_doc,
    clippy::result_large_err
)]

use std::io::Cursor;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use aws_sdk_s3::config::{Credentials, Region};
use aws_sdk_s3::primitives::ByteStream;
use aws_sdk_s3::Client;
use clap::Parser;
use futures::stream::{self, StreamExt};
use image::ImageReader;
use thiserror::Error;
use tokio::fs;
use tokio::io::AsyncWriteExt;

#[derive(Debug, Error)]
enum AppError {
    #[error("http: {0}")]
    Http(#[from] reqwest::Error),
    #[error("image: {0}")]
    Image(#[from] image::ImageError),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("s3: {0}")]
    S3(String),
    #[error("size format: {0}")]
    Size(String),
    #[error("join: {0}")]
    Join(#[from] tokio::task::JoinError),
}

#[derive(Parser)]
struct Args {
    #[arg(long)]
    files: PathBuf,

    #[arg(long, value_parser = parse_size)]
    resize: (u32, u32),

    #[arg(long, default_value_t = false)]
    keep_aspect: bool,

    #[arg(long, default_value_t = 8)]
    concurrency: usize,
}

fn parse_size(s: &str) -> Result<(u32, u32), AppError> {
    let (w, h) = s
        .split_once('x')
        .ok_or_else(|| AppError::Size("expected WIDTHxHEIGHT".into()))?;
    let w = w
        .parse::<u32>()
        .map_err(|e| AppError::Size(e.to_string()))?;
    let h = h
        .parse::<u32>()
        .map_err(|e| AppError::Size(e.to_string()))?;
    Ok((w, h))
}

trait FileUploader: Send + Sync {
    fn upload<'a>(
        &'a self,
        name: &'a str,
        data: Vec<u8>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), AppError>> + Send + 'a>>;
}

struct FsUploader {
    base_path: PathBuf,
}

impl FileUploader for FsUploader {
    fn upload<'a>(
        &'a self,
        name: &'a str,
        data: Vec<u8>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), AppError>> + Send + 'a>>
    {
        Box::pin(async move {
            fs::create_dir_all(&self.base_path).await?;
            let path = self.base_path.join(name);
            let mut file = fs::File::create(&path).await?;
            file.write_all(&data).await?;
            file.flush().await?;
            Ok(())
        })
    }
}

struct S3Uploader {
    client: Client,
    bucket: String,
}

impl FileUploader for S3Uploader {
    fn upload<'a>(
        &'a self,
        name: &'a str,
        data: Vec<u8>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), AppError>> + Send + 'a>>
    {
        Box::pin(async move {
            let body = ByteStream::from(data);
            self.client
                .put_object()
                .bucket(&self.bucket)
                .key(name)
                .body(body)
                .send()
                .await
                .map_err(|e| AppError::S3(format!("{e:?}")))?;
            Ok(())
        })
    }
}

fn build_uploader() -> Arc<dyn FileUploader> {
    match std::env::var("MYME_UPLOADER").as_deref() {
        Ok("s3") => {
            let access_key = std::env::var("AWS_ACCESS_KEY_ID").expect("AWS_ACCESS_KEY_ID not set");
            let secret_key =
                std::env::var("AWS_SECRET_ACCESS_KEY").expect("AWS_SECRET_ACCESS_KEY not set");
            let bucket = std::env::var("S3_BUCKET").expect("S3_BUCKET not set");
            let region_str =
                std::env::var("S3_REGION").unwrap_or_else(|_| "us-east-1".to_string());
            let endpoint = std::env::var("S3_ENDPOINT").ok();

            let creds = Credentials::new(access_key, secret_key, None, None, "env");
            let mut builder = aws_sdk_s3::Config::builder()
                .credentials_provider(creds)
                .region(Region::new(region_str))
                .behavior_version_latest();

            if let Some(ep) = endpoint {
                builder = builder.endpoint_url(ep).force_path_style(true);
            }

            let client = Client::from_conf(builder.build());
            Arc::new(S3Uploader { client, bucket })
        }
        _ => {
            let path = std::env::var("MYME_FILES_PATH").unwrap_or_else(|_| ".".to_string());
            Arc::new(FsUploader {
                base_path: PathBuf::from(path),
            })
        }
    }
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

async fn fetch_bytes(client: &reqwest::Client, line: &str) -> Result<Vec<u8>, AppError> {
    if line.starts_with("http://") || line.starts_with("https://") {
        let resp = client.get(line).send().await?;
        Ok(resp.bytes().await?.to_vec())
    } else {
        Ok(fs::read(line).await?)
    }
}

fn derive_name(line: &str, index: usize) -> String {
    let raw = line.split('?').next().unwrap_or(line);
    let last = raw.rsplit(['/', '\\']).next().unwrap_or("");
    let stem = std::path::Path::new(last)
        .file_stem()
        .and_then(|s| s.to_str())
        .filter(|s| !s.is_empty())
        .unwrap_or("image");
    format!("{index}_{stem}.png")
}

fn cpu_process(
    bytes: Vec<u8>,
    width: u32,
    height: u32,
    keep_aspect: bool,
) -> Result<Vec<u8>, AppError> {
    let img = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()?
        .decode()?;

    let resized = if keep_aspect {
        img.resize(width, height, image::imageops::FilterType::Lanczos3)
    } else {
        img.resize_exact(width, height, image::imageops::FilterType::Lanczos3)
    };

    let mut buf = Cursor::new(Vec::new());
    resized.write_to(&mut buf, image::ImageFormat::Png)?;
    Ok(buf.into_inner())
}

async fn process_image(
    client: reqwest::Client,
    uploader: Arc<dyn FileUploader>,
    line: String,
    width: u32,
    height: u32,
    keep_aspect: bool,
    index: usize,
) {
    let bytes = match fetch_bytes(&client, &line).await {
        Ok(b) => b,
        Err(e) => {
            eprintln!("[{line}] fetch error: {e}");
            return;
        }
    };

    let processed = match tokio::task::spawn_blocking(move || {
        cpu_process(bytes, width, height, keep_aspect)
    })
    .await
    {
        Ok(Ok(data)) => data,
        Ok(Err(e)) => {
            eprintln!("[{line}] cpu error: {e}");
            return;
        }
        Err(e) => {
            eprintln!("[{line}] join error: {e}");
            return;
        }
    };

    let name = derive_name(&line, index);
    match uploader.upload(&name, processed).await {
        Ok(()) => println!("[{line}] uploaded as {name}"),
        Err(e) => eprintln!("[{line}] upload error: {e}"),
    }
}

#[tokio::main]
async fn main() {
    let args = Args::parse();
    let (width, height) = args.resize;
    let uploader = build_uploader();

    let raw = fs::read(&args.files)
        .await
        .expect("failed to read input file");
    let content = decode_text(&raw);

    let lines: Vec<(usize, String)> = content
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(String::from)
        .enumerate()
        .collect();

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .connect_timeout(Duration::from_secs(10))
        .build()
        .expect("failed to build http client");

    println!(
        "[mode] async, concurrency={}, files={}",
        args.concurrency,
        lines.len()
    );

    let started = Instant::now();

    let tasks = stream::iter(lines.into_iter().map(|(i, line)| {
        let client = client.clone();
        let uploader = Arc::clone(&uploader);
        process_image(client, uploader, line, width, height, args.keep_aspect, i)
    }))
    .buffer_unordered(args.concurrency);

    tasks.for_each(|()| async {}).await;

    let elapsed = started.elapsed();
    println!("[time] elapsed: {:.3}s", elapsed.as_secs_f64());
}
