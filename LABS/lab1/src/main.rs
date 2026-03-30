use std::fs;
use std::io::Cursor;
use std::path::{Path, PathBuf};

use clap::Parser;
use image::ImageReader;

#[derive(Parser)]
struct Args {
    #[arg(long)]
    files: PathBuf,

    #[arg(long, value_parser = parse_size)]
    resize: (u32, u32),
}

fn parse_size(s: &str) -> Result<(u32, u32), String> {
    let (w, h) = s.split_once('x').ok_or("expected WIDTHxHEIGHT")?;
    let w = w.parse::<u32>().map_err(|e| e.to_string())?;
    let h = h.parse::<u32>().map_err(|e| e.to_string())?;
    Ok((w, h))
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

fn output_dir() -> PathBuf {
    PathBuf::from(std::env::var("MYME_FILES_PATH").unwrap_or_else(|_| ".".to_string()))
}

fn load_image_from_url(url: &str) -> Result<image::DynamicImage, String> {
    let bytes = reqwest::blocking::get(url)
        .map_err(|e| e.to_string())?
        .bytes()
        .map_err(|e| e.to_string())?;
    ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|e| e.to_string())?
        .decode()
        .map_err(|e| e.to_string())
}

fn load_image_from_path(path: &str) -> Result<image::DynamicImage, String> {
    ImageReader::open(path)
        .map_err(|e| e.to_string())?
        .decode()
        .map_err(|e| e.to_string())
}

fn process_line(line: &str, width: u32, height: u32, out_dir: &Path, index: usize) {
    let img = if line.starts_with("http://") || line.starts_with("https://") {
        load_image_from_url(line)
    } else {
        load_image_from_path(line)
    };

    match img {
        Err(e) => eprintln!("[{}] error: {}", line, e),
        Ok(img) => {
            let resized = img.resize_exact(width, height, image::imageops::FilterType::Lanczos3);
            let out_path = out_dir.join(format!("{}.png", index));
            match resized.save(&out_path) {
                Ok(_) => println!("[{}] saved to {}", line, out_path.display()),
                Err(e) => eprintln!("[{}] save error: {}", line, e),
            }
        }
    }
}

fn main() {
    let args = Args::parse();
    let (width, height) = args.resize;
    let out_dir = output_dir();

    fs::create_dir_all(&out_dir).expect("failed to create output directory");

    let raw = fs::read(&args.files).expect("failed to read input file");
    let content = decode_text(&raw);

    for (i, line) in content.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        process_line(line, width, height, &out_dir, i);
    }
}
