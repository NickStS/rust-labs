//! Image editor CLI.

#![warn(
    missing_docs,
    missing_crate_level_docs,
    clippy::missing_panics_doc,
    clippy::missing_errors_doc,
    clippy::result_large_err
)]

use std::fs;
use std::io::Cursor;
use std::path::PathBuf;
use std::time::Duration;

use aws_sdk_s3::config::{Credentials, Region};
use aws_sdk_s3::primitives::ByteStream;
use aws_sdk_s3::Client;
use clap::Parser;
use image::ImageReader;
use thiserror::Error;

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
}

#[derive(Parser)]
struct Args {
    #[arg(long)]
    files: PathBuf,

    #[arg(long, value_parser = parse_size)]
    resize: (u32, u32),

    #[arg(long, default_value_t = false)]
    keep_aspect: bool,
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

trait FileUploader {
    fn upload(&self, name: &str, data: &[u8]) -> Result<(), AppError>;
}

struct FsUploader {
    base_path: PathBuf,
}

impl FileUploader for FsUploader {
    fn upload(&self, name: &str, data: &[u8]) -> Result<(), AppError> {
        fs::create_dir_all(&self.base_path)?;
        fs::write(self.base_path.join(name), data)?;
        Ok(())
    }
}

struct S3Uploader {
    client: Client,
    bucket: String,
}

impl FileUploader for S3Uploader {
    fn upload(&self, name: &str, data: &[u8]) -> Result<(), AppError> {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| AppError::S3(e.to_string()))?;
        let body = ByteStream::from(data.to_vec());
        rt.block_on(async {
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

fn build_uploader() -> Box<dyn FileUploader> {
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
            Box::new(S3Uploader { client, bucket })
        }
        _ => {
            let path = std::env::var("MYME_FILES_PATH").unwrap_or_else(|_| ".".to_string());
            Box::new(FsUploader {
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
        String::from_utf16_lossy(&words).to_string()
    } else if raw.starts_with(&[0xFE, 0xFF]) {
        let words: Vec<u16> = raw[2..]
            .chunks_exact(2)
            .map(|b| u16::from_be_bytes([b[0], b[1]]))
            .collect();
        String::from_utf16_lossy(&words).to_string()
    } else if raw.starts_with(&[0xEF, 0xBB, 0xBF]) {
        String::from_utf8_lossy(&raw[3..]).to_string()
    } else {
        String::from_utf8_lossy(raw).to_string()
    }
}

fn http_client() -> Result<reqwest::blocking::Client, AppError> {
    Ok(reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(30))
        .connect_timeout(Duration::from_secs(10))
        .build()?)
}

fn load_image_from_url(url: &str) -> Result<image::DynamicImage, AppError> {
    let bytes = http_client()?.get(url).send()?.bytes()?;
    Ok(ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()?
        .decode()?)
}

fn load_image_from_path(path: &str) -> Result<image::DynamicImage, AppError> {
    Ok(ImageReader::open(path)?.decode()?)
}

fn derive_name(line: &str, index: usize) -> String {
    let raw = if let Some(stripped) = line.split('?').next() {
        stripped
    } else {
        line
    };
    let last = raw.rsplit(['/', '\\']).next().unwrap_or("");
    let stem = std::path::Path::new(last)
        .file_stem()
        .and_then(|s| s.to_str())
        .filter(|s| !s.is_empty())
        .unwrap_or("image");
    format!("{index}_{stem}.png")
}

fn process_line(
    line: &str,
    width: u32,
    height: u32,
    keep_aspect: bool,
    uploader: &dyn FileUploader,
    index: usize,
) {
    let img = if line.starts_with("http://") || line.starts_with("https://") {
        load_image_from_url(line)
    } else {
        load_image_from_path(line)
    };

    match img {
        Err(e) => eprintln!("[{line}] error: {e}"),
        Ok(img) => {
            let resized = if keep_aspect {
                img.resize(width, height, image::imageops::FilterType::Lanczos3)
            } else {
                img.resize_exact(width, height, image::imageops::FilterType::Lanczos3)
            };
            let name = derive_name(line, index);
            let mut buf = Cursor::new(Vec::new());
            if let Err(e) = resized.write_to(&mut buf, image::ImageFormat::Png) {
                eprintln!("[{line}] encode error: {e}");
                return;
            }
            match uploader.upload(&name, &buf.into_inner()) {
                Ok(()) => println!("[{line}] uploaded as {name}"),
                Err(e) => eprintln!("[{line}] upload error: {e}"),
            }
        }
    }
}

fn main() {
    let args = Args::parse();
    let (width, height) = args.resize;
    let uploader = build_uploader();

    let raw = fs::read(&args.files).expect("failed to read input file");
    let content = decode_text(&raw);

    for (i, line) in content.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        process_line(line, width, height, args.keep_aspect, uploader.as_ref(), i);
    }
}
