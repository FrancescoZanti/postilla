use rusqlite::{Connection, Result};
use std::fs;
use std::path::PathBuf;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct Session {
    pub id: i64,
    pub session_type: String,
    pub title: String,
    pub created_at: String,
    pub updated_at: String,
    pub status: String,
    pub file_path: Option<String>,
    pub transcript: Option<String>,
    pub summary: Option<String>,
}

pub fn init_db(app_data_dir: &PathBuf) -> Result<Connection> {
    if !app_data_dir.exists() {
        fs::create_dir_all(app_data_dir).expect("Failed to create app data directory");
    }

    let db_path = app_data_dir.join("postilla.db");
    let conn = Connection::open(db_path)?;

    // Create the sessions table
    conn.execute(
        "CREATE TABLE IF NOT EXISTS sessions (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            session_type TEXT NOT NULL,
            title TEXT NOT NULL,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            status TEXT NOT NULL
        )",
        (),
    )?;

    // Migrations
    let _ = conn.execute("ALTER TABLE sessions ADD COLUMN file_path TEXT", ());
    let _ = conn.execute("ALTER TABLE sessions ADD COLUMN transcript TEXT", ());
    let _ = conn.execute("ALTER TABLE sessions ADD COLUMN summary TEXT", ());

    Ok(conn)
}
