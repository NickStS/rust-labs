use std::env;
use std::error::Error;
use std::fs;
use std::io::{self, Read};
use std::path::PathBuf;

use chrono::{DateTime, Utc};
use clap::Parser;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Snippet {
    name: String,
    content: String,
    created_at: DateTime<Utc>,
}

trait SnippetStorage {
    fn create(&mut self, name: &str, content: &str) -> Result<(), Box<dyn Error>>;
    fn read(&self, name: &str) -> Result<Snippet, Box<dyn Error>>;
    fn delete(&mut self, name: &str) -> Result<(), Box<dyn Error>>;
    fn list(&self) -> Result<Vec<Snippet>, Box<dyn Error>>;
}

struct JsonStorage {
    path: PathBuf,
}

impl JsonStorage {
    fn new(path: PathBuf) -> Self {
        Self { path }
    }

    fn load_all(&self) -> Result<Vec<Snippet>, Box<dyn Error>> {
        if !self.path.exists() {
            return Ok(Vec::new());
        }
        let data = fs::read_to_string(&self.path)?;
        if data.trim().is_empty() {
            return Ok(Vec::new());
        }
        let snippets: Vec<Snippet> = serde_json::from_str(&data)?;
        Ok(snippets)
    }

    fn save_all(&self, snippets: &[Snippet]) -> Result<(), Box<dyn Error>> {
        if let Some(parent) = self.path.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent)?;
            }
        }
        let data = serde_json::to_string_pretty(snippets)?;
        fs::write(&self.path, data)?;
        Ok(())
    }
}

impl SnippetStorage for JsonStorage {
    fn create(&mut self, name: &str, content: &str) -> Result<(), Box<dyn Error>> {
        let mut snippets = self.load_all()?;
        snippets.retain(|s| s.name != name);
        snippets.push(Snippet {
            name: name.to_string(),
            content: content.to_string(),
            created_at: Utc::now(),
        });
        self.save_all(&snippets)
    }

    fn read(&self, name: &str) -> Result<Snippet, Box<dyn Error>> {
        let snippets = self.load_all()?;
        snippets
            .into_iter()
            .find(|s| s.name == name)
            .ok_or_else(|| format!("snippet '{name}' not found").into())
    }

    fn delete(&mut self, name: &str) -> Result<(), Box<dyn Error>> {
        let mut snippets = self.load_all()?;
        let before = snippets.len();
        snippets.retain(|s| s.name != name);
        if snippets.len() == before {
            return Err(format!("snippet '{name}' not found").into());
        }
        self.save_all(&snippets)
    }

    fn list(&self) -> Result<Vec<Snippet>, Box<dyn Error>> {
        let mut snippets = self.load_all()?;
        snippets.sort_by_key(|s| s.created_at);
        Ok(snippets)
    }
}

struct SqliteStorage {
    conn: Connection,
}

impl SqliteStorage {
    fn new(path: PathBuf) -> Result<Self, Box<dyn Error>> {
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent)?;
            }
        }
        let conn = Connection::open(path)?;
        conn.execute(
            r#"
            CREATE TABLE IF NOT EXISTS snippets (
                name TEXT PRIMARY KEY,
                content TEXT NOT NULL,
                created_at TEXT NOT NULL
            )
            "#,
            [],
        )?;
        Ok(Self { conn })
    }
}

impl SnippetStorage for SqliteStorage {
    fn create(&mut self, name: &str, content: &str) -> Result<(), Box<dyn Error>> {
        let created_at = Utc::now().to_rfc3339();
        self.conn.execute(
            r#"
            INSERT INTO snippets (name, content, created_at)
            VALUES (?1, ?2, ?3)
            ON CONFLICT(name) DO UPDATE SET
                content = excluded.content,
                created_at = excluded.created_at
            "#,
            params![name, content, &created_at],
        )?;
        Ok(())
    }

    fn read(&self, name: &str) -> Result<Snippet, Box<dyn Error>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT name, content, created_at
            FROM snippets
            WHERE name = ?1
            "#,
        )?;
        let row_opt = stmt
            .query_row(params![name], |row| {
                let name: String = row.get(0)?;
                let content: String = row.get(1)?;
                let created_at_str: String = row.get(2)?;
                let dt = chrono::DateTime::parse_from_rfc3339(&created_at_str).map_err(|e| {
                    rusqlite::Error::FromSqlConversionFailure(
                        2,
                        rusqlite::types::Type::Text,
                        Box::new(e),
                    )
                })?;
                Ok(Snippet {
                    name,
                    content,
                    created_at: dt.with_timezone(&Utc),
                })
            })
            .optional()?;
        match row_opt {
            Some(sn) => Ok(sn),
            None => Err(format!("snippet '{name}' not found").into()),
        }
    }

    fn delete(&mut self, name: &str) -> Result<(), Box<dyn Error>> {
        let rows = self.conn.execute(
            r#"
            DELETE FROM snippets
            WHERE name = ?1
            "#,
            params![name],
        )?;
        if rows == 0 {
            return Err(format!("snippet '{name}' not found").into());
        }
        Ok(())
    }

    fn list(&self) -> Result<Vec<Snippet>, Box<dyn Error>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT name, content, created_at
            FROM snippets
            ORDER BY created_at
            "#,
        )?;
        let rows = stmt.query_map([], |row| {
            let name: String = row.get(0)?;
            let content: String = row.get(1)?;
            let created_at_str: String = row.get(2)?;
            let dt = chrono::DateTime::parse_from_rfc3339(&created_at_str).map_err(|e| {
                rusqlite::Error::FromSqlConversionFailure(
                    2,
                    rusqlite::types::Type::Text,
                    Box::new(e),
                )
            })?;
            Ok(Snippet {
                name,
                content,
                created_at: dt.with_timezone(&Utc),
            })
        })?;
        let mut result = Vec::new();
        for item in rows {
            let sn = item?;
            result.push(sn);
        }
        Ok(result)
    }
}

enum StorageConfig {
    Json(PathBuf),
    Sqlite(PathBuf),
}

fn parse_storage_spec(spec: &str) -> Result<StorageConfig, Box<dyn Error>> {
    let (kind, path) = spec
        .split_once(':')
        .ok_or_else(|| "expected format KIND:/path/to/file".to_string())?;
    let path = PathBuf::from(path);
    match kind {
        "JSON" => Ok(StorageConfig::Json(path)),
        "SQLITE" => Ok(StorageConfig::Sqlite(path)),
        _ => Err("unknown storage kind, expected JSON or SQLITE".to_string().into()),
    }
}

fn build_storage() -> Result<Box<dyn SnippetStorage>, Box<dyn Error>> {
    if let Ok(spec) = env::var("SNIPPETS_APP_STORAGE") {
        let cfg = parse_storage_spec(&spec)?;
        return match cfg {
            StorageConfig::Json(path) => Ok(Box::new(JsonStorage::new(path))),
            StorageConfig::Sqlite(path) => Ok(Box::new(SqliteStorage::new(path)?)),
        };
    }
    Ok(Box::new(JsonStorage::new(PathBuf::from("snippets.json"))))
}

#[derive(Debug, Parser)]
#[command(name = "snippets-app")]
#[command(about = "Simple CLI for storing code snippets")]
struct Cli {
    #[arg(long)]
    name: Option<String>,
    #[arg(long)]
    read: Option<String>,
    #[arg(long)]
    delete: Option<String>,
    #[arg(long)]
    list: bool,
}

fn run() -> Result<(), Box<dyn Error>> {
    let cli = Cli::parse();
    let mut storage = build_storage()?;

    let mut actions = 0;
    if cli.name.is_some() {
        actions += 1;
    }
    if cli.read.is_some() {
        actions += 1;
    }
    if cli.delete.is_some() {
        actions += 1;
    }
    if cli.list {
        actions += 1;
    }
    if actions != 1 {
        return Err("exactly one of --name / --read / --delete / --list must be provided".into());
    }

    if let Some(name) = cli.name {
        let mut buf = String::new();
        io::stdin().read_to_string(&mut buf)?;
        storage.create(&name, &buf)?;
        println!("Snippet '{name}' created");
        return Ok(());
    }

    if let Some(name) = cli.read {
        let snippet = storage.read(&name)?;
        println!("{}", snippet.content);
        return Ok(());
    }

    if let Some(name) = cli.delete {
        storage.delete(&name)?;
        println!("Snippet '{name}' deleted");
        return Ok(());
    }

    if cli.list {
        let snippets = storage.list()?;
        for s in snippets {
            println!("{} ({})", s.name, s.created_at);
        }
        return Ok(());
    }

    Ok(())
}

fn main() {
    if let Err(err) = run() {
        eprintln!("Error: {err}");
        std::process::exit(1);
    }
}
