#![warn(clippy::missing_errors_doc, clippy::result_large_err)]

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use filesindex_core::{FileIndex, JsonIndex, SqliteIndex};

#[derive(Parser)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    Add {
        #[arg(long)]
        path: String,
        #[arg(long, value_delimiter = ',')]
        tags: Vec<String>,
    },
    Get {
        #[arg(long, value_delimiter = ',')]
        tags: Vec<String>,
    },
}

fn open_index() -> Result<Box<dyn FileIndex>> {
    let env = std::env::var("FILES_INDEX_PATH")
        .unwrap_or_else(|_| "json:.files_index.json".to_string());

    let (kind, path) = env
        .split_once(':')
        .context("FILES_INDEX_PATH must be in format type:path")?;

    match kind {
        "json" => Ok(Box::new(JsonIndex::open(path))),
        "sqlite" => Ok(Box::new(SqliteIndex::open(path))),
        other => anyhow::bail!("unknown storage type: {}", other),
    }
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let mut index = open_index()?;

    match cli.command {
        Command::Add { path, tags } => {
            index.add(&path, &tags).context("failed to add entry")?;
            println!("added: {} [{:?}]", path, tags);
        }
        Command::Get { tags } => {
            let entries = index.get(&tags).context("failed to query index")?;
            if entries.is_empty() {
                println!("no files found");
            } else {
                for e in entries {
                    println!("{} [{}]", e.path, e.tags.join(", "));
                }
            }
        }
    }

    Ok(())
}
