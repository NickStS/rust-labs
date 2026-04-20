//! Core library for file indexing with JSON and SQLite backends.

#![warn(clippy::missing_errors_doc, clippy::result_large_err)]

use std::fs;
use std::path::Path;

use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum IndexError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("sqlite: {0}")]
    Sqlite(#[from] rusqlite::Error),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileEntry {
    pub path: String,
    pub tags: Vec<String>,
}

pub trait FileIndex {
    /// # Errors
    /// Returns [`IndexError`] if the storage operation fails.
    fn add(&mut self, path: &str, tags: &[String]) -> Result<(), IndexError>;

    /// # Errors
    /// Returns [`IndexError`] if the storage operation fails.
    fn get(&self, tags: &[String]) -> Result<Vec<FileEntry>, IndexError>;

    /// # Errors
    /// Returns [`IndexError`] if the storage operation fails.
    fn remove(&mut self, path: &str) -> Result<bool, IndexError>;
}

#[derive(Serialize, Deserialize, Default)]
struct JsonData {
    entries: Vec<FileEntry>,
}

pub struct JsonIndex {
    path: String,
    data: JsonData,
}

impl JsonIndex {
    /// # Errors
    /// Returns [`IndexError`] if reading or parsing the file fails.
    pub fn open(path: &str) -> Result<Self, IndexError> {
        let data = if Path::new(path).exists() {
            let raw = fs::read_to_string(path)?;
            serde_json::from_str(&raw).unwrap_or_default()
        } else {
            JsonData::default()
        };
        Ok(JsonIndex { path: path.to_string(), data })
    }

    fn save(&self) -> Result<(), IndexError> {
        let json = serde_json::to_string_pretty(&self.data)?;
        fs::write(&self.path, json)?;
        Ok(())
    }
}

impl FileIndex for JsonIndex {
    fn add(&mut self, path: &str, tags: &[String]) -> Result<(), IndexError> {
        if let Some(entry) = self.data.entries.iter_mut().find(|e| e.path == path) {
            for tag in tags {
                if !entry.tags.contains(tag) {
                    entry.tags.push(tag.clone());
                }
            }
        } else {
            self.data.entries.push(FileEntry {
                path: path.to_string(),
                tags: tags.to_vec(),
            });
        }
        self.save()
    }

    fn get(&self, tags: &[String]) -> Result<Vec<FileEntry>, IndexError> {
        let result = self
            .data
            .entries
            .iter()
            .filter(|e| tags.iter().all(|t| e.tags.contains(t)))
            .cloned()
            .collect();
        Ok(result)
    }

    fn remove(&mut self, path: &str) -> Result<bool, IndexError> {
        let len_before = self.data.entries.len();
        self.data.entries.retain(|e| e.path != path);
        if self.data.entries.len() < len_before {
            self.save()?;
            Ok(true)
        } else {
            Ok(false)
        }
    }
}

pub struct SqliteIndex {
    conn: Connection,
}

impl SqliteIndex {
    /// # Errors
    /// Returns [`IndexError`] if opening the database or creating tables fails.
    pub fn open(path: &str) -> Result<Self, IndexError> {
        let conn = Connection::open(path)?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS entries (id INTEGER PRIMARY KEY, path TEXT UNIQUE NOT NULL);
             CREATE TABLE IF NOT EXISTS tags (entry_id INTEGER NOT NULL, tag TEXT NOT NULL,
               FOREIGN KEY(entry_id) REFERENCES entries(id));",
        )?;
        Ok(SqliteIndex { conn })
    }
}

impl FileIndex for SqliteIndex {
    fn add(&mut self, path: &str, tags: &[String]) -> Result<(), IndexError> {
        self.conn
            .execute("INSERT OR IGNORE INTO entries (path) VALUES (?1)", [path])?;
        let id: i64 = self.conn.query_row(
            "SELECT id FROM entries WHERE path = ?1",
            [path],
            |r| r.get(0),
        )?;
        for tag in tags {
            let exists: bool = self.conn.query_row(
                "SELECT COUNT(*) FROM tags WHERE entry_id = ?1 AND tag = ?2",
                rusqlite::params![id, tag],
                |r| r.get::<_, i64>(0),
            )? > 0;
            if !exists {
                self.conn.execute(
                    "INSERT INTO tags (entry_id, tag) VALUES (?1, ?2)",
                    rusqlite::params![id, tag],
                )?;
            }
        }
        Ok(())
    }

    fn get(&self, tags: &[String]) -> Result<Vec<FileEntry>, IndexError> {
        if tags.is_empty() {
            return Ok(Vec::new());
        }

        let placeholders: Vec<String> = (1..=tags.len()).map(|i| format!("?{i}")).collect();
        let sql = format!(
            "SELECT e.id, e.path FROM entries e
             JOIN tags t ON t.entry_id = e.id
             WHERE t.tag IN ({})
             GROUP BY e.id
             HAVING COUNT(DISTINCT t.tag) = ?{}",
            placeholders.join(", "),
            tags.len() + 1
        );

        let mut stmt = self.conn.prepare(&sql)?;
        let mut params: Vec<Box<dyn rusqlite::types::ToSql>> =
            tags.iter().map(|t| Box::new(t.clone()) as Box<dyn rusqlite::types::ToSql>).collect();
        params.push(Box::new(tags.len() as i64));

        let rows: Vec<(i64, String)> = stmt
            .query_map(rusqlite::params_from_iter(&params), |r| Ok((r.get(0)?, r.get(1)?)))?
            .filter_map(|r| r.ok())
            .collect();

        let mut result = Vec::new();
        for (id, path) in rows {
            let mut tag_stmt = self
                .conn
                .prepare("SELECT tag FROM tags WHERE entry_id = ?1")?;
            let entry_tags: Vec<String> = tag_stmt
                .query_map([id], |r| r.get(0))?
                .filter_map(|r| r.ok())
                .collect();
            result.push(FileEntry { path, tags: entry_tags });
        }
        Ok(result)
    }

    fn remove(&mut self, path: &str) -> Result<bool, IndexError> {
        let id: Option<i64> = self
            .conn
            .query_row("SELECT id FROM entries WHERE path = ?1", [path], |r| r.get(0))
            .ok();
        if let Some(id) = id {
            self.conn.execute("DELETE FROM tags WHERE entry_id = ?1", [id])?;
            self.conn.execute("DELETE FROM entries WHERE id = ?1", [id])?;
            Ok(true)
        } else {
            Ok(false)
        }
    }
}
