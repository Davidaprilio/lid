//! SQLite persistence for the download list and queues (`store.py` in 1.x): the same file, schema and
//! JSON blobs, so a 1.x `downloads.db` opens unchanged.

use std::path::Path;
use std::sync::Mutex;

use indexmap::IndexMap;
use rusqlite::{Connection, params};

use crate::models::Download;
use crate::queue::QueueRecord;

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS downloads (id INTEGER PRIMARY KEY, data TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS queues (name TEXT PRIMARY KEY, data TEXT NOT NULL);
";

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("download list: {0}")]
    Sql(#[from] rusqlite::Error),
    #[error("download list: {0}")]
    Io(#[from] std::io::Error),
}

/// A record that could not be read; it stays in the database untouched.
#[derive(Debug, Clone, PartialEq)]
pub struct BadRow {
    pub key: String,
    pub error: String,
}

pub struct Store {
    db: Mutex<Connection>,
}

impl Store {
    /// Open (and create) the database at `path`; `":memory:"` keeps it in memory.
    pub fn open(path: &Path) -> Result<Store, StoreError> {
        let db = if path == Path::new(":memory:") {
            Connection::open_in_memory()?
        } else {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            Connection::open(path)?
        };
        db.execute_batch(SCHEMA)?;
        Ok(Store { db: Mutex::new(db) })
    }

    pub fn in_memory() -> Result<Store, StoreError> {
        Store::open(Path::new(":memory:"))
    }

    fn db(&self) -> std::sync::MutexGuard<'_, Connection> {
        self.db.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Every download in id order, and the rows that could not be read. 1.x stopped at the first bad
    /// row; here the others still load and the bad ones are left in the file.
    pub fn load_downloads(&self) -> Result<(Vec<Download>, Vec<BadRow>), StoreError> {
        let db = self.db();
        let mut stmt = db.prepare("SELECT id, data FROM downloads ORDER BY id")?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))?;
        let (mut ok, mut bad) = (Vec::new(), Vec::new());
        for row in rows {
            let (id, data) = row?;
            match serde_json::from_str::<Download>(&data) {
                Ok(d) => ok.push(d),
                Err(e) => bad.push(BadRow {
                    key: id.to_string(),
                    error: e.to_string(),
                }),
            }
        }
        Ok((ok, bad))
    }

    pub fn save_download(&self, d: &Download) -> Result<(), StoreError> {
        self.save_downloads(std::slice::from_ref(d))
    }

    /// Insert or replace, all in one transaction.
    pub fn save_downloads(&self, items: &[Download]) -> Result<(), StoreError> {
        let mut db = self.db();
        let tx = db.transaction()?;
        {
            let mut stmt =
                tx.prepare("INSERT OR REPLACE INTO downloads (id, data) VALUES (?1, ?2)")?;
            for d in items {
                stmt.execute(params![d.id, d.to_json().to_string()])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    pub fn delete_downloads(&self, ids: &[i64]) -> Result<(), StoreError> {
        let mut db = self.db();
        let tx = db.transaction()?;
        {
            let mut stmt = tx.prepare("DELETE FROM downloads WHERE id = ?1")?;
            for id in ids {
                stmt.execute(params![id])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// One more than the highest id in the list.
    pub fn next_id(&self) -> Result<i64, StoreError> {
        Ok(self
            .db()
            .query_row("SELECT COALESCE(MAX(id), 0) + 1 FROM downloads", [], |r| {
                r.get(0)
            })?)
    }

    /// Every saved queue by name, in table order, and the rows that could not be read.
    #[allow(clippy::type_complexity)]
    pub fn load_queues(&self) -> Result<(IndexMap<String, QueueRecord>, Vec<BadRow>), StoreError> {
        let db = self.db();
        let mut stmt = db.prepare("SELECT name, data FROM queues")?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
        let (mut ok, mut bad) = (IndexMap::new(), Vec::new());
        for row in rows {
            let (name, data) = row?;
            match serde_json::from_str::<QueueRecord>(&data) {
                Ok(q) => {
                    ok.insert(name, q);
                }
                Err(e) => bad.push(BadRow {
                    key: name,
                    error: e.to_string(),
                }),
            }
        }
        Ok((ok, bad))
    }

    pub fn save_queue(&self, name: &str, record: &QueueRecord) -> Result<(), StoreError> {
        let data = serde_json::to_string(record).expect("a queue always serialises");
        self.db().execute(
            "INSERT OR REPLACE INTO queues (name, data) VALUES (?1, ?2)",
            params![name, data],
        )?;
        Ok(())
    }

    pub fn delete_queue(&self, name: &str) -> Result<(), StoreError> {
        self.db()
            .execute("DELETE FROM queues WHERE name = ?1", params![name])?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{Segment, Status};

    #[test]
    fn save_load_delete() {
        let tmp = tempfile::tempdir().unwrap();
        let store = Store::open(&tmp.path().join("sub/downloads.db")).unwrap();
        assert_eq!(store.next_id().unwrap(), 1);
        let mut a = Download::new(1, "https://x/a", "a", "/d");
        a.segments = vec![Segment::new(0, 10, 5)];
        a.status = Status::Paused;
        let b = Download::new(5, "https://x/b", "b", "/d");
        store.save_downloads(&[b.clone(), a.clone()]).unwrap();
        assert_eq!(store.next_id().unwrap(), 6);
        let (loaded, bad) = store.load_downloads().unwrap();
        assert!(bad.is_empty());
        assert_eq!(loaded, vec![a.clone(), b]);
        store.delete_downloads(&[5]).unwrap();
        assert_eq!(store.load_downloads().unwrap().0, vec![a]);
    }

    #[test]
    fn bad_rows_are_reported_not_fatal() {
        let store = Store::in_memory().unwrap();
        store
            .save_download(&Download::new(1, "u", "f", "/d"))
            .unwrap();
        store
            .db()
            .execute(
                "INSERT INTO downloads (id, data) VALUES (2, '{\"id\": 2, \"status\": \"weird\"}')",
                [],
            )
            .unwrap();
        let (ok, bad) = store.load_downloads().unwrap();
        assert_eq!(ok.len(), 1);
        assert_eq!(bad.len(), 1);
        assert_eq!(bad[0].key, "2");
    }

    #[test]
    fn queues() {
        let store = Store::in_memory().unwrap();
        let mut q = QueueRecord::default();
        q.schedule.max_parallel = 3;
        store.save_queue("Nightly", &q).unwrap();
        store
            .save_queue("Main download queue", &QueueRecord::default())
            .unwrap();
        let (loaded, _) = store.load_queues().unwrap();
        assert_eq!(loaded["Nightly"].schedule.max_parallel, 3);
        store.delete_queue("Nightly").unwrap();
        assert_eq!(store.load_queues().unwrap().0.len(), 1);
    }
}
