#![deny(missing_docs)]
#![deny(rustdoc::broken_intra_doc_links)]
#![deny(rustdoc::missing_crate_level_docs)]
#![deny(unreachable_pub)]
#![deny(clippy::missing_panics_doc)]
#![deny(clippy::clone_on_ref_ptr)]
#![deny(clippy::similar_names)]


//! `snippets-app` — CLI приложение для хранения и чтения сниппетов.
//!
//! Поддерживает JSON и SQLite хранилища, логирование и загрузку сниппетов по URL.


use std::env;
use std::fs;
use std::io::{self, Read};
use std::path::PathBuf;

use chrono::{DateTime, Utc};
use clap::Parser;
use reqwest::blocking::Client;
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tracing::{debug, info};
use tracing_subscriber::fmt::MakeWriter;

#[derive(Debug, Error)]
enum AppError {
    #[error("io error at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("json error at {path}: {source}")]
    Json {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },
    #[error("sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("http error when requesting {url}: {source}")]
    Http {
        url: String,
        #[source]
        source: reqwest::Error,
    },
    #[error("snippet '{0}' not found")]
    NotFound(String),
    #[error("invalid storage config: {0}")]
    InvalidConfig(String),
    #[error("invalid arguments: {0}")]
    InvalidArgs(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Snippet {
    name: String,
    content: String,
    created_at: DateTime<Utc>,
}

trait SnippetStorage {
    fn create(&mut self, name: &str, content: &str) -> Result<(), AppError>;
    fn read(&self, name: &str) -> Result<Snippet, AppError>;
    fn delete(&mut self, name: &str) -> Result<(), AppError>;
    fn list(&self) -> Result<Vec<Snippet>, AppError>;
}

struct JsonStorage {
    path: PathBuf,
}

impl JsonStorage {
    fn new(path: PathBuf) -> Self {
        Self { path }
    }

    fn load_all(&self) -> Result<Vec<Snippet>, AppError> {
        if !self.path.exists() {
            return Ok(Vec::new());
        }
        let data = fs::read_to_string(&self.path).map_err(|source| AppError::Io {
            path: self.path.clone(),
            source,
        })?;
        if data.trim().is_empty() {
            return Ok(Vec::new());
        }
        let snippets: Vec<Snippet> =
            serde_json::from_str(&data).map_err(|source| AppError::Json {
                path: self.path.clone(),
                source,
            })?;
        Ok(snippets)
    }

    fn save_all(&self, snippets: &[Snippet]) -> Result<(), AppError> {
        if let Some(parent) = self.path.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent).map_err(|source| AppError::Io {
                    path: parent.to_path_buf(),
                    source,
                })?;
            }
        }
        let data = serde_json::to_string_pretty(snippets).map_err(|source| AppError::Json {
            path: self.path.clone(),
            source,
        })?;
        fs::write(&self.path, data).map_err(|source| AppError::Io {
            path: self.path.clone(),
            source,
        })?;
        Ok(())
    }
}

impl SnippetStorage for JsonStorage {
    fn create(&mut self, name: &str, content: &str) -> Result<(), AppError> {
        info!("create snippet '{name}'");
        let mut snippets = self.load_all()?;
        snippets.retain(|s| s.name != name);
        snippets.push(Snippet {
            name: name.to_string(),
            content: content.to_string(),
            created_at: Utc::now(),
        });
        self.save_all(&snippets)
    }

    fn read(&self, name: &str) -> Result<Snippet, AppError> {
        info!("read snippet '{name}'");
        let snippets = self.load_all()?;
        snippets
            .into_iter()
            .find(|s| s.name == name)
            .ok_or_else(|| AppError::NotFound(name.to_string()))
    }

    fn delete(&mut self, name: &str) -> Result<(), AppError> {
        info!("delete snippet '{name}'");
        let mut snippets = self.load_all()?;
        let before = snippets.len();
        snippets.retain(|s| s.name != name);
        if snippets.len() == before {
            return Err(AppError::NotFound(name.to_string()));
        }
        self.save_all(&snippets)
    }

    fn list(&self) -> Result<Vec<Snippet>, AppError> {
        info!("list snippets");
        let mut snippets = self.load_all()?;
        snippets.sort_by_key(|s| s.created_at);
        Ok(snippets)
    }
}

struct SqliteStorage {
    conn: Connection,
}

impl SqliteStorage {
    fn new(path: PathBuf) -> Result<Self, AppError> {
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent).map_err(|source| AppError::Io {
                    path: parent.to_path_buf(),
                    source,
                })?;
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
    fn create(&mut self, name: &str, content: &str) -> Result<(), AppError> {
        info!("create snippet '{name}'");
        let created_at = Utc::now().to_rfc3339();
        self.conn.execute(
            r#"
            INSERT INTO snippets (name, content, created_at)
            VALUES (?1, ?2, ?3)
            ON CONFLICT(name) DO UPDATE SET
                content = excluded.content,
                created_at = excluded.created_at
            "#,
            params![name, content, created_at],
        )?;
        Ok(())
    }

    fn read(&self, name: &str) -> Result<Snippet, AppError> {
        info!("read snippet '{name}'");
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
            None => Err(AppError::NotFound(name.to_string())),
        }
    }

    fn delete(&mut self, name: &str) -> Result<(), AppError> {
        info!("delete snippet '{name}'");
        let rows = self.conn.execute(
            r#"
            DELETE FROM snippets
            WHERE name = ?1
            "#,
            params![name],
        )?;
        if rows == 0 {
            return Err(AppError::NotFound(name.to_string()));
        }
        Ok(())
    }

    fn list(&self) -> Result<Vec<Snippet>, AppError> {
        info!("list snippets");
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

fn parse_storage_spec(spec: &str) -> Result<StorageConfig, AppError> {
    let (kind, path) = spec
        .split_once(':')
        .ok_or_else(|| AppError::InvalidConfig("expected format KIND:/path/to/file".into()))?;
    let path_buf = PathBuf::from(path);
    match kind {
        "JSON" => Ok(StorageConfig::Json(path_buf)),
        "SQLITE" => Ok(StorageConfig::Sqlite(path_buf)),
        other => Err(AppError::InvalidConfig(format!(
            "unknown storage kind '{other}', expected JSON or SQLITE"
        ))),
    }
}

fn build_storage() -> Result<Box<dyn SnippetStorage>, AppError> {
    if let Ok(spec) = env::var("SNIPPETS_APP_STORAGE") {
        let cfg = parse_storage_spec(&spec)?;
        return match cfg {
            StorageConfig::Json(path) => Ok(Box::new(JsonStorage::new(path))),
            StorageConfig::Sqlite(path) => Ok(Box::new(SqliteStorage::new(path)?)),
        };
    }
    Ok(Box::new(JsonStorage::new(PathBuf::from("snippets.json"))))
}

#[derive(Clone)]
struct FileMakeWriter {
    path: Option<PathBuf>,
}

struct FileWriter {
    path: Option<PathBuf>,
}

impl<'a> MakeWriter<'a> for FileMakeWriter {
    type Writer = FileWriter;

    fn make_writer(&'a self) -> Self::Writer {
        FileWriter {
            path: self.path.clone(),
        }
    }
}

impl std::io::Write for FileWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        use std::io::Write;
        if let Some(path) = &self.path {
            if let Ok(mut f) = fs::OpenOptions::new().create(true).append(true).open(path) {
                let _ = f.write_all(buf);
            }
        }
        std::io::stderr().write_all(buf)?;
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        std::io::stderr().flush()
    }
}

fn init_tracing() {
    let level_str = env::var("SNIPPETS_APP_LOG_LEVEL").unwrap_or_else(|_| "info".to_string());
    let lower = level_str.to_ascii_lowercase();
    let level = match lower.as_str() {
        "error" => tracing::Level::ERROR,
        "warn" => tracing::Level::WARN,
        "debug" => tracing::Level::DEBUG,
        "trace" => tracing::Level::TRACE,
        _ => tracing::Level::INFO,
    };
    let path = env::var("SNIPPETS_APP_LOG_PATH").ok().map(PathBuf::from);
    let make_writer = FileMakeWriter { path };
    let _ = tracing_subscriber::fmt()
        .with_max_level(level)
        .with_writer(make_writer)
        .with_target(false)
        .try_init();
}

fn download_snippet(url: &str) -> Result<String, AppError> {
    let client = Client::new();
    let resp = client.get(url).send().map_err(|source| AppError::Http {
        url: url.to_string(),
        source,
    })?;
    let resp = resp.error_for_status().map_err(|source| AppError::Http {
        url: url.to_string(),
        source,
    })?;
    let body = resp.text().map_err(|source| AppError::Http {
        url: url.to_string(),
        source,
    })?;
    Ok(body)
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
    #[arg(long)]
    download: Option<String>,
}

fn run() -> Result<(), AppError> {
    init_tracing();
    let cli = Cli::parse();
    debug!("cli args: {:?}", cli);
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
        return Err(AppError::InvalidArgs(
            "exactly one of --name / --read / --delete / --list must be provided".into(),
        ));
    }
    if cli.download.is_some() && cli.name.is_none() {
        return Err(AppError::InvalidArgs(
            "--download can be used only together with --name".into(),
        ));
    }
    if let Some(name) = cli.name {
        let content = if let Some(url) = cli.download.as_deref() {
            info!("downloading snippet from '{url}'");
            download_snippet(url)?
        } else {
            let mut buf = String::new();
            io::stdin()
                .read_to_string(&mut buf)
                .map_err(|source| AppError::Io {
                    path: PathBuf::from("<stdin>"),
                    source,
                })?;
            buf
        };
        storage.create(&name, &content)?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::{Mutex, OnceLock};
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    static ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

    fn env_guard() -> std::sync::MutexGuard<'static, ()> {
        ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap()
    }

    fn unique_temp_path(ext: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let mut p = std::env::temp_dir();
        p.push(format!("snippets_app_test_{nanos}.{ext}"));
        p
    }

    fn read_file_to_string(path: &PathBuf) -> String {
        fs::read_to_string(path).unwrap_or_else(|_| "".to_string())
    }

    #[test]
    fn json_storage_crud_and_persist() {
        let path = unique_temp_path("json");
        let mut storage = JsonStorage::new(path.clone());

        storage.create("a", "one").unwrap();
        let sn = storage.read("a").unwrap();
        assert_eq!(sn.name, "a");
        assert_eq!(sn.content, "one");

        let listed = storage.list().unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].name, "a");

        let file_data = read_file_to_string(&path);
        assert!(!file_data.trim().is_empty());
        let parsed: Vec<Snippet> = serde_json::from_str(&file_data).unwrap();
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].name, "a");

        storage.delete("a").unwrap();
        assert!(matches!(storage.read("a"), Err(AppError::NotFound(_))));

        let _ = fs::remove_file(path);
    }

    #[test]
    fn json_storage_overwrite_same_name() {
        let path = unique_temp_path("json");
        let mut storage = JsonStorage::new(path.clone());

        storage.create("x", "v1").unwrap();
        storage.create("x", "v2").unwrap();

        let sn = storage.read("x").unwrap();
        assert_eq!(sn.content, "v2");

        let listed = storage.list().unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].name, "x");
        assert_eq!(listed[0].content, "v2");

        let _ = fs::remove_file(path);
    }

    #[test]
    fn json_storage_list_sorted_by_created_at() {
        let path = unique_temp_path("json");
        let mut storage = JsonStorage::new(path.clone());

        storage.create("first", "1").unwrap();
        std::thread::sleep(Duration::from_millis(10));
        storage.create("second", "2").unwrap();

        let listed = storage.list().unwrap();
        assert_eq!(listed.len(), 2);
        assert_eq!(listed[0].name, "first");
        assert_eq!(listed[1].name, "second");
        assert!(listed[0].created_at <= listed[1].created_at);

        let _ = fs::remove_file(path);
    }

    #[test]
    fn sqlite_storage_crud() {
        let path = unique_temp_path("sqlite");
        let mut storage = SqliteStorage::new(path.clone()).unwrap();

        storage.create("a", "one").unwrap();
        let sn = storage.read("a").unwrap();
        assert_eq!(sn.name, "a");
        assert_eq!(sn.content, "one");

        let listed = storage.list().unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].name, "a");

        storage.delete("a").unwrap();
        assert!(matches!(storage.read("a"), Err(AppError::NotFound(_))));

        let _ = fs::remove_file(path);
    }

    #[test]
    fn sqlite_storage_overwrite_same_name() {
        let path = unique_temp_path("sqlite");
        let mut storage = SqliteStorage::new(path.clone()).unwrap();

        storage.create("x", "v1").unwrap();
        storage.create("x", "v2").unwrap();

        let sn = storage.read("x").unwrap();
        assert_eq!(sn.content, "v2");

        let listed = storage.list().unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].name, "x");
        assert_eq!(listed[0].content, "v2");

        let _ = fs::remove_file(path);
    }

    #[test]
    fn parse_storage_spec_json_and_sqlite() {
        let cfg = parse_storage_spec("JSON:/tmp/a.json").unwrap();
        match cfg {
            StorageConfig::Json(p) => assert!(p.to_string_lossy().contains("a.json")),
            _ => panic!("expected JSON"),
        }

        let cfg = parse_storage_spec("SQLITE:/tmp/a.sqlite").unwrap();
        match cfg {
            StorageConfig::Sqlite(p) => assert!(p.to_string_lossy().contains("a.sqlite")),
            _ => panic!("expected SQLITE"),
        }

        assert!(parse_storage_spec("BAD:/tmp/x").is_err());
        assert!(parse_storage_spec("NO_COLON").is_err());
    }

    #[test]
    fn build_storage_uses_env_storage_spec() {
        let _g = env_guard();
        let old = std::env::var("SNIPPETS_APP_STORAGE").ok();

        let path = unique_temp_path("json");
        unsafe {
            std::env::set_var(
                "SNIPPETS_APP_STORAGE",
                format!("JSON:{}", path.to_string_lossy()),
            );
        }

        let mut s = build_storage().unwrap();
        s.create("a", "one").unwrap();
        let sn = s.read("a").unwrap();
        assert_eq!(sn.content, "one");

        if let Some(v) = old {
            unsafe {
                std::env::set_var("SNIPPETS_APP_STORAGE", v);
            }
        } else {
            unsafe {
                std::env::remove_var("SNIPPETS_APP_STORAGE");
            }
        }

        let _ = fs::remove_file(path);
    }

    #[test]
    fn download_snippet_from_local_http() {
        use tiny_http::{Response, Server};

        let server = Server::http("127.0.0.1:0").unwrap();
        let addr = server.server_addr().to_string();
        let url = format!("http://{addr}/");

        let handle = std::thread::spawn(move || {
            if let Ok(req) = server.recv() {
                let resp = Response::from_string("hello_from_http");
                let _ = req.respond(resp);
            }
        });

        let body = download_snippet(&url).unwrap();
        assert_eq!(body, "hello_from_http");

        let _ = handle.join();
    }

    #[test]
    fn cli_parsing_download_requires_name() {
        let res = Cli::try_parse_from(["snippets-app", "--download", "http://example.com"]);
        assert!(res.is_ok());
        let cli = res.unwrap();
        assert!(cli.download.is_some());
        assert!(cli.name.is_none());
    }
}
