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
    fn add(&mut self, path: &str, tags: &[String]) -> Result<(), IndexError>;
    fn get(&self, tags: &[String]) -> Result<Vec<FileEntry>, IndexError>;
}

// JSON backend

#[derive(Serialize, Deserialize, Default)]
struct JsonData {
    entries: Vec<FileEntry>,
}

pub struct JsonIndex {
    path: String,
    data: JsonData,
}

impl JsonIndex {
    pub fn open(path: &str) -> Self {
        let data = if Path::new(path).exists() {
            let raw = fs::read_to_string(path).unwrap_or_default();
            serde_json::from_str(&raw).unwrap_or_default()
        } else {
            JsonData::default()
        };
        JsonIndex { path: path.to_string(), data }
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
}

// SQLite backend

pub struct SqliteIndex {
    conn: Connection,
}

impl SqliteIndex {
    pub fn open(path: &str) -> Self {
        let conn = Connection::open(path).expect("failed to open sqlite db");
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS entries (id INTEGER PRIMARY KEY, path TEXT UNIQUE NOT NULL);
             CREATE TABLE IF NOT EXISTS tags (entry_id INTEGER NOT NULL, tag TEXT NOT NULL,
               FOREIGN KEY(entry_id) REFERENCES entries(id));",
        )
        .expect("failed to init schema");
        SqliteIndex { conn }
    }
}

impl FileIndex for SqliteIndex {
    fn add(&mut self, path: &str, tags: &[String]) -> Result<(), IndexError> {
        self.conn.execute(
            "INSERT OR IGNORE INTO entries (path) VALUES (?1)",
            [path],
        )?;
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
        let mut stmt = self.conn.prepare(
            "SELECT e.path FROM entries e
             JOIN tags t ON t.entry_id = e.id
             WHERE t.tag IN (SELECT tag FROM tags WHERE entry_id = e.id)
             GROUP BY e.id",
        )?;

        let paths: Vec<String> = stmt
            .query_map([], |r| r.get(0))?
            .filter_map(|r| r.ok())
            .collect();

        let mut result = Vec::new();
        for path in paths {
            let id: i64 = self.conn.query_row(
                "SELECT id FROM entries WHERE path = ?1",
                [&path],
                |r| r.get(0),
            )?;
            let mut tag_stmt = self
                .conn
                .prepare("SELECT tag FROM tags WHERE entry_id = ?1")?;
            let entry_tags: Vec<String> = tag_stmt
                .query_map([id], |r| r.get(0))?
                .filter_map(|r| r.ok())
                .collect();
            if tags.iter().all(|t| entry_tags.contains(t)) {
                result.push(FileEntry { path, tags: entry_tags });
            }
        }
        Ok(result)
    }
}
