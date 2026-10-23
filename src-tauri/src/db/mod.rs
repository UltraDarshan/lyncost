//! SQLite connection management: opening, pragmas, migrations and seed data.

mod migrations;
mod seed;

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

use rusqlite::Connection;

use crate::util::CmdResult;

pub use seed::seed_defaults;

pub const DB_FILE: &str = "lyncost.db";
/// The app was called DhanKhata before it was renamed; carry old data over.
const LEGACY_DB_FILE: &str = "dhankhata.db";

pub struct AppState {
    db: Mutex<Connection>,
    pub data_dir: PathBuf,
    /// Set when the database could not be opened; the UI shows it instead of the lock screen.
    pub startup_error: Option<String>,
}

impl AppState {
    pub fn new(data_dir: PathBuf) -> Self {
        let (conn, startup_error) = match open_in_dir(&data_dir) {
            Ok(conn) => (conn, None),
            Err(e) => {
                eprintln!("[lyncost] database startup failed: {e}");
                let placeholder = Connection::open_in_memory().expect("in-memory sqlite");
                (placeholder, Some(e))
            }
        };
        Self { db: Mutex::new(conn), data_dir, startup_error }
    }

    pub fn conn(&self) -> CmdResult<MutexGuard<'_, Connection>> {
        if let Some(e) = &self.startup_error {
            return Err(e.clone());
        }
        self.db.lock().map_err(|_| "Database is unavailable (lock poisoned)".to_string())
    }

    pub fn db_path(&self) -> PathBuf {
        self.data_dir.join(DB_FILE)
    }
}

pub fn open_in_dir(data_dir: &Path) -> Result<Connection, String> {
    fs::create_dir_all(data_dir).map_err(|e| format!("Failed to create app data directory: {e}"))?;
    let path = data_dir.join(DB_FILE);
    let legacy = data_dir.join(LEGACY_DB_FILE);
    if !path.exists() && legacy.exists() {
        fs::rename(&legacy, &path).map_err(|e| format!("Failed to migrate legacy database: {e}"))?;
    }
    open(&path)
}

pub fn open(path: &Path) -> Result<Connection, String> {
    let mut conn = Connection::open(path)
        .map_err(|e| format!("Failed to open SQLite database at {}: {e}", path.display()))?;
    configure(&conn)?;
    migrations::run(&mut conn)?;
    seed_defaults(&conn)?;
    Ok(conn)
}

fn configure(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(
        "PRAGMA foreign_keys = ON;
         PRAGMA journal_mode = WAL;
         PRAGMA synchronous = NORMAL;
         PRAGMA busy_timeout = 5000;
         PRAGMA cache_size = -20000;
         PRAGMA temp_store = MEMORY;",
    )
    .map_err(|e| format!("Failed to configure SQLite pragmas: {e}"))
}

#[cfg(test)]
pub fn open_memory() -> Connection {
    let mut conn = Connection::open_in_memory().unwrap();
    conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
    migrations::run(&mut conn).unwrap();
    seed_defaults(&conn).unwrap();
    conn
}

