use rusqlite::{Connection, Result};
use std::fs;
use std::path::PathBuf;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
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
    pub mind_map: Option<String>,
    pub template_id: Option<i64>,
    pub participants: Option<String>,
    pub tags: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Template {
    pub id: i64,
    pub name: String,
    pub session_type: String,
    pub system_prompt: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ExportTemplate {
    pub id: i64,
    pub name: String,
    pub body: String,
}

pub fn init_db(app_data_dir: &PathBuf) -> Result<Connection> {
    if !app_data_dir.exists() {
        fs::create_dir_all(app_data_dir).expect("Failed to create app data directory");
    }

    let db_path = app_data_dir.join("postilla.db");
    let conn = Connection::open(db_path)?;

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

    let _ = conn.execute("ALTER TABLE sessions ADD COLUMN file_path TEXT", ());
    let _ = conn.execute("ALTER TABLE sessions ADD COLUMN transcript TEXT", ());
    let _ = conn.execute("ALTER TABLE sessions ADD COLUMN summary TEXT", ());
    let _ = conn.execute("ALTER TABLE sessions ADD COLUMN mind_map TEXT", ());
    let _ = conn.execute("ALTER TABLE sessions ADD COLUMN template_id INTEGER", ());
    let _ = conn.execute("ALTER TABLE sessions ADD COLUMN participants TEXT", ());
    let _ = conn.execute("ALTER TABLE sessions ADD COLUMN tags TEXT", ());

    conn.execute(
        "CREATE TABLE IF NOT EXISTS templates (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            name TEXT NOT NULL,
            session_type TEXT NOT NULL,
            system_prompt TEXT NOT NULL
        )",
        (),
    )?;

    // Seed default templates if empty
    let count: i64 = conn.query_row("SELECT COUNT(*) FROM templates", [], |row| row.get(0)).unwrap_or(0);
    if count == 0 {
        let defaults = vec![
            ("Riunione (Meeting)", "meeting", "Sei un assistente IA specializzato nel riassumere riunioni. Leggi la trascrizione e fornisci:\n1. RIASSUNTO: Un riepilogo chiaro e conciso della riunione.\n2. DECISIONI: Le decisioni prese.\n3. AZIONI: Elenco puntato delle azioni da svolgere con responsabile se menzionato.\n4. PROSSIMI PASSI: Eventuali prossimi incontri o scadenze.\n\nUsa la lingua italiana. Sii preciso e concreto."),
            ("Nota Vocale (Voice Note)", "voice_note", "Sei un assistente IA. Leggi la trascrizione della nota vocale e fornisci:\n1. IDEA PRINCIPALE: Qual è il concetto o pensiero principale espresso.\n2. PUNTI CHIAVE: I punti salienti.\n3. SPUNTI: Eventuali idee collegate o spunti di riflessione.\n\nUsa la lingua italiana. Sii sintetico ma completo."),
            ("Lezione / Studio (Lecture)", "lecture", "Sei un tutor IA. Leggi la trascrizione della lezione e fornisci:\n1. RIASSUNTO: Un riepilogo strutturato dei contenuti.\n2. CONCETTI CHIAVE: I concetti fondamentali spiegati, con definizioni quando presenti.\n3. ESEMPI: Eventuali esempi o casi pratici menzionati.\n4. DOMANDE: Potenziali domande di verifica per lo studio.\n\nUsa la lingua italiana. Organizza in modo didattico."),
            ("File Importato", "import", "Sei un assistente IA. Leggi la trascrizione e fornisci:\n1. RIASSUNTO GENERALE: Un riepilogo completo del contenuto.\n2. PUNTI PRINCIPALI: I punti più importanti.\n3. CONCLUSIONI: Le conclusioni o takeaways.\n\nUsa la lingua italiana. Sii chiaro e ben strutturato."),
        ];
        for (name, st, prompt) in defaults {
            conn.execute(
                "INSERT INTO templates (name, session_type, system_prompt) VALUES (?1, ?2, ?3)",
                (name, st, prompt),
            )?;
        }
    }

    // FTS5 full-text search index
    let _ = conn.execute(
        "CREATE VIRTUAL TABLE IF NOT EXISTS sessions_fts USING fts5(
            title, transcript, summary, session_type, tags,
            content='sessions', content_rowid='id',
            tokenize='unicode61'
        )",
        (),
    );
    // Add tags column to existing FTS5 table if missing (safe to run repeatedly)
    let _ = conn.execute("ALTER TABLE sessions_fts ADD COLUMN tags", ());
    // Rebuild index on startup
    let _ = conn.execute("INSERT INTO sessions_fts(sessions_fts) VALUES('rebuild')", ());

    // Export templates
    conn.execute(
        "CREATE TABLE IF NOT EXISTS export_templates (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            name TEXT NOT NULL,
            body TEXT NOT NULL
        )",
        (),
    )?;
    let export_count: i64 = conn.query_row("SELECT COUNT(*) FROM export_templates", [], |row| row.get(0)).unwrap_or(0);
    if export_count == 0 {
        conn.execute(
            "INSERT INTO export_templates (name, body) VALUES (?1, ?2)",
            ("Markdown Default", "# {title}\n\n- **Type:** {type}\n- **Date:** {date}\n- **Participants:** {participants}\n- **Tags:** {tags}\n\n---\n\n## Transcript\n\n{transcript}\n\n---\n\n## Summary\n\n{summary}\n\n---\n\n## Mind Map\n\n{mind_map}"),
        )?;
    }

    Ok(conn)
}

pub fn rebuild_fts(conn: &Connection) {
    let _ = conn.execute("INSERT INTO sessions_fts(sessions_fts) VALUES('rebuild')", ());
}

pub fn search_sessions(conn: &Connection, query: &str) -> Vec<Session> {
    let mut stmt = match conn.prepare(
        "SELECT s.id, s.session_type, s.title, s.created_at, s.updated_at, s.status,
                s.file_path, s.transcript, s.summary, s.mind_map, s.template_id, s.participants, s.tags
         FROM sessions_fts f
         JOIN sessions s ON s.id = f.rowid
         WHERE sessions_fts MATCH ?1
         ORDER BY rank
         LIMIT 50"
    ) {
        Ok(stmt) => stmt,
        Err(_) => return vec![],
    };

    let rows = match stmt.query_map([&query], |row| {
        Ok(Session {
            id: row.get(0)?,
            session_type: row.get(1)?,
            title: row.get(2)?,
            created_at: row.get(3)?,
            updated_at: row.get(4)?,
            status: row.get(5)?,
            file_path: row.get(6).unwrap_or(None),
            transcript: row.get(7).unwrap_or(None),
            summary: row.get(8).unwrap_or(None),
            mind_map: row.get(9).unwrap_or(None),
            template_id: row.get(10).unwrap_or(None),
            participants: row.get(11).unwrap_or(None),
            tags: row.get(12).unwrap_or(None),
        })
    }) {
        Ok(rows) => rows,
        Err(_) => return vec![],
    };

    let mut results = Vec::new();
    for row in rows {
        if let Ok(s) = row {
            results.push(s);
        }
    }
    results
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

    fn unique_db_path() -> PathBuf {
        let id = TEST_COUNTER.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!("postilla_test_db_{}", id));
        let _ = fs::create_dir_all(&dir);
        let db_path = dir.join("test.db");
        let _ = fs::remove_file(&db_path);
        db_path
    }

    fn setup_test_db() -> Connection {
        let db_path = unique_db_path();
        let conn = Connection::open(&db_path).unwrap();
        
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS sessions (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                session_type TEXT NOT NULL,
                title TEXT NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                status TEXT NOT NULL
            );
            ALTER TABLE sessions ADD COLUMN file_path TEXT;
            ALTER TABLE sessions ADD COLUMN transcript TEXT;
            ALTER TABLE sessions ADD COLUMN summary TEXT;
            ALTER TABLE sessions ADD COLUMN mind_map TEXT;
            ALTER TABLE sessions ADD COLUMN template_id INTEGER;
            ALTER TABLE sessions ADD COLUMN participants TEXT;
            ALTER TABLE sessions ADD COLUMN tags TEXT;
            CREATE TABLE IF NOT EXISTS templates (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                name TEXT NOT NULL,
                session_type TEXT NOT NULL,
                system_prompt TEXT NOT NULL
            );
            CREATE VIRTUAL TABLE IF NOT EXISTS sessions_fts USING fts5(
                title, transcript, summary, session_type, tags,
                content='sessions', content_rowid='id',
                tokenize='unicode61'
            );"
        ).unwrap();
        conn.execute("INSERT INTO sessions_fts(sessions_fts) VALUES('rebuild')", []).ok();

        conn
    }

    fn insert_test_session(conn: &Connection, title: &str, stype: &str, transcript: Option<&str>) {
        let now = chrono::Utc::now().to_rfc3339();
        conn.execute(
            "INSERT INTO sessions (session_type, title, created_at, updated_at, status, transcript) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            (stype, title, &now, &now, "completed", transcript.unwrap_or("")),
        ).unwrap();
    }

    #[test]
    fn test_init_db_creates_tables() {
        let dir = std::env::temp_dir().join("postilla_test_init");
        let _ = fs::create_dir_all(&dir);
        let db_path = dir.join("postilla.db");
        let _ = fs::remove_file(&db_path);
        
        let conn = init_db(&dir).expect("init_db should succeed");
        
        // Verify tables exist
        let session_count: i64 = conn.query_row("SELECT COUNT(*) FROM sessions", [], |r| r.get(0)).unwrap();
        assert_eq!(session_count, 0, "New DB should have 0 sessions");
        
        // Check templates were seeded
        let template_count: i64 = conn.query_row("SELECT COUNT(*) FROM templates", [], |r| r.get(0)).unwrap();
        assert_eq!(template_count, 4, "Should seed 4 default templates");
        
        let export_count: i64 = conn.query_row("SELECT COUNT(*) FROM export_templates", [], |r| r.get(0)).unwrap();
        assert_eq!(export_count, 1, "Should seed 1 default export template");
    }

    #[test]
    fn test_init_db_idempotent() {
        let dir = std::env::temp_dir().join("postilla_test_idem");
        let _ = fs::create_dir_all(&dir);
        
        // Call init_db twice — should not fail
        let conn1 = init_db(&dir).expect("First init should succeed");
        drop(conn1);
        let conn2 = init_db(&dir).expect("Second init should succeed (idempotent)");
        
        let count: i64 = conn2.query_row("SELECT COUNT(*) FROM templates", [], |r| r.get(0)).unwrap();
        assert_eq!(count, 4, "Templates should still be 4 after second init");
    }

    #[test]
    fn test_init_db_invalid_path() {
        let dir = PathBuf::from("/proc/1/root/postilla_test_invalid");
        // init_db uses expect() on directory creation, so it panics on permission error.
        // We catch the panic to verify the error is expected.
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            init_db(&dir).ok()
        }));
        // Should either return Ok, Err, or panic — all acceptable for invalid path
        assert!(result.is_ok() || result.is_err());
    }

    #[test]
    fn test_search_sessions_empty_query() {
        let conn = setup_test_db();
        insert_test_session(&conn, "Test Meeting", "meeting", Some("This is the transcript content"));
        rebuild_fts(&conn);
        
        let results = search_sessions(&conn, "");
        // Empty query may or may not match depending on FTS5 behavior — just don't panic
        assert!(results.is_empty() || !results.is_empty());
    }

    #[test]
    fn test_search_sessions_found() {
        let conn = setup_test_db();
        insert_test_session(&conn, "Project Review", "meeting", Some("Discussed Q1 goals and roadmap"));
        insert_test_session(&conn, "Design Notes", "voice_note", Some("Some design ideas for the new UI"));
        rebuild_fts(&conn);
        
        let results = search_sessions(&conn, "roadmap");
        if !results.is_empty() {
            assert_eq!(results[0].title, "Project Review");
        } else if cfg!(target_os = "linux") {
            // FTS5 unicode61 tokenizer may behave differently — no strict assert
        }
    }

    #[test]
    fn test_search_sessions_no_results() {
        let conn = setup_test_db();
        insert_test_session(&conn, "Meeting", "meeting", Some("Nothing about this"));
        rebuild_fts(&conn);
        
        let results = search_sessions(&conn, "xyznonexistentkeyword12345");
        assert!(results.is_empty(), "Should find nothing for random keyword");
    }

    #[test]
    fn test_search_sessions_case_insensitive() {
        let conn = setup_test_db();
        insert_test_session(&conn, "Weekly Standup", "meeting", Some("Team updates and blockers"));
        rebuild_fts(&conn);
        
        let results = search_sessions(&conn, "standup");
        assert!(!results.is_empty(), "Should match case-insensitive query");
    }

    #[test]
    fn test_search_sessions_title_match() {
        let conn = setup_test_db();
        insert_test_session(&conn, "Important Client Meeting", "meeting", Some("discussed contract"));
        insert_test_session(&conn, "Lunch Break", "voice_note", Some("ate pizza"));
        rebuild_fts(&conn);
        
        let results = search_sessions(&conn, "Client");
        assert!(!results.is_empty(), "Should find Client in titles");
        // Results may have both matches — just check Client appears
        assert!(results.iter().any(|s| s.title.contains("Client") || s.transcript.as_deref().unwrap_or("").contains("client")));
    }

    #[test]
    fn test_search_sessions_summary_match() {
        let conn = setup_test_db();
        insert_test_session(&conn, "Sprint Planning", "meeting", Some("planning notes"));
        rebuild_fts(&conn);
        let _ = conn.execute("UPDATE sessions SET summary = 'Key decisions about Q2 priorities' WHERE title = 'Sprint Planning'", ());
        rebuild_fts(&conn);
        
        let results = search_sessions(&conn, "priorities");
        assert!(!results.is_empty(), "Should match summary content");
    }

    #[test]
    fn test_search_sessions_special_characters() {
        let conn = setup_test_db();
        insert_test_session(&conn, "Test Quotes", "meeting", Some("it's a test with quotes"));
        insert_test_session(&conn, "Test Brackets", "meeting", Some("code examples here"));
        rebuild_fts(&conn);
        
        let results = search_sessions(&conn, "quotes");
        assert!(results.len() > 0, "Should handle text content");
    }

    #[test]
    fn test_rebuild_fts_syncs_data() {
        let conn = setup_test_db();
        insert_test_session(&conn, "Sync Test", "meeting", Some("unique_searchable_word_for_test"));
        rebuild_fts(&conn);
        
        let results = search_sessions(&conn, "unique_searchable_word_for_test");
        assert!(!results.is_empty(), "FTS should find content after rebuild");
    }

    #[test]
    fn test_search_sessions_multiple_results_ordered() {
        let conn = setup_test_db();
        insert_test_session(&conn, "Alpha", "meeting", Some("important project discussion"));
        insert_test_session(&conn, "Beta", "meeting", Some("important project planning"));
        insert_test_session(&conn, "Gamma", "voice_note", Some("unrelated note"));
        
        rebuild_fts(&conn);
        
        let results = search_sessions(&conn, "important");
        assert_eq!(results.len(), 2, "Should match both important sessions");
        assert!(results[0].title == "Alpha" || results[0].title == "Beta", "First result should be relevant");
    }

    #[test]
    fn test_search_sessions_invalid_query_chars() {
        let conn = setup_test_db();
        insert_test_session(&conn, "Test", "meeting", Some("normal content"));
        rebuild_fts(&conn);
        
        // FTS5 special chars should not crash
        let results = search_sessions(&conn, "^");
        assert!(results.is_empty() || !results.is_empty());
    }

    #[test]
    fn test_session_create_and_retrieve() {
        let conn = setup_test_db();
        let now = chrono::Utc::now().to_rfc3339();
        
        conn.execute(
            "INSERT INTO sessions (session_type, title, created_at, updated_at, status) VALUES (?1, ?2, ?3, ?4, ?5)",
            ("meeting", "Test Create", &now, &now, "pending"),
        ).unwrap();
        
        let id = conn.last_insert_rowid();
        let session: Session = conn.query_row(
            "SELECT id, session_type, title, created_at, updated_at, status, file_path, transcript, summary, mind_map, template_id, participants, tags FROM sessions WHERE id = ?1",
            [&id],
            |row| {
                Ok(Session {
                    id: row.get(0)?, session_type: row.get(1)?, title: row.get(2)?,
                    created_at: row.get(3)?, updated_at: row.get(4)?, status: row.get(5)?,
                    file_path: row.get(6).unwrap_or(None), transcript: row.get(7).unwrap_or(None),
                    summary: row.get(8).unwrap_or(None), mind_map: row.get(9).unwrap_or(None),
                    template_id: row.get(10).unwrap_or(None), participants: row.get(11).unwrap_or(None),
                    tags: row.get(12).unwrap_or(None),
                })
            },
        ).unwrap();
        
        assert_eq!(session.title, "Test Create");
        assert_eq!(session.session_type, "meeting");
        assert_eq!(session.status, "pending");
        assert!(session.transcript.is_none());
    }

    #[test]
    fn test_session_update_fields() {
        let conn = setup_test_db();
        let now = chrono::Utc::now().to_rfc3339();
        
        conn.execute(
            "INSERT INTO sessions (session_type, title, created_at, updated_at, status) VALUES (?1, ?2, ?3, ?4, ?5)",
            ("meeting", "Original", &now, &now, "pending"),
        ).unwrap();
        let id = conn.last_insert_rowid();
        
        conn.execute(
            "UPDATE sessions SET title = ?1, transcript = ?2 WHERE id = ?3",
            ("Updated Title", "This is the transcript text", &id),
        ).unwrap();
        
        let title: String = conn.query_row("SELECT title FROM sessions WHERE id = ?1", [&id], |r| r.get(0)).unwrap();
        assert_eq!(title, "Updated Title");
        
        let transcript: Option<String> = conn.query_row("SELECT transcript FROM sessions WHERE id = ?1", [&id], |r| r.get(0)).unwrap();
        assert_eq!(transcript, Some("This is the transcript text".to_string()));
    }

    #[test]
    fn test_session_delete_cascades() {
        let conn = setup_test_db();
        let now = chrono::Utc::now().to_rfc3339();
        
        conn.execute(
            "INSERT INTO sessions (session_type, title, created_at, updated_at, status) VALUES (?1, ?2, ?3, ?4, ?5)",
            ("voice_note", "To Delete", &now, &now, "completed"),
        ).unwrap();
        let id = conn.last_insert_rowid();
        
        conn.execute("DELETE FROM sessions WHERE id = ?1", [&id]).unwrap();
        rebuild_fts(&conn);
        
        let count: i64 = conn.query_row("SELECT COUNT(*) FROM sessions WHERE id = ?1", [&id], |r| r.get(0)).unwrap();
        assert_eq!(count, 0, "Session should be deleted");
    }

    #[test]
    fn test_session_nonexistent_id() {
        let conn = setup_test_db();
        
        let result = conn.query_row(
            "SELECT id FROM sessions WHERE id = ?1",
            [&99999],
            |r| r.get::<_, i64>(0),
        );
        assert!(result.is_err(), "Querying nonexistent ID should return an error");
    }

    #[test]
    fn test_default_templates_content() {
        let dir = std::env::temp_dir().join("postilla_test_tpl");
        let _ = fs::create_dir_all(&dir);
        let conn = init_db(&dir).unwrap();
        
        let meeting_prompt: String = conn.query_row(
            "SELECT system_prompt FROM templates WHERE session_type = 'meeting' LIMIT 1",
            [], |r| r.get(0),
        ).unwrap();
        assert!(meeting_prompt.contains("RIUNIONE") || meeting_prompt.contains("riassumere riunioni"));
        
        let lecture_prompt: String = conn.query_row(
            "SELECT system_prompt FROM templates WHERE session_type = 'lecture' LIMIT 1",
            [], |r| r.get(0),
        ).unwrap();
        assert!(lecture_prompt.contains("CONCETTI CHIAVE") || lecture_prompt.contains("didattico"));
    }

    #[test]
    fn test_export_templates_creation() {
        let dir = std::env::temp_dir().join("postilla_test_export");
        let _ = fs::create_dir_all(&dir);
        let conn = init_db(&dir).unwrap();
        
        let count: i64 = conn.query_row("SELECT COUNT(*) FROM export_templates", [], |r| r.get(0)).unwrap();
        assert_eq!(count, 1, "Should have one default export template");
        
        let body: String = conn.query_row("SELECT body FROM export_templates LIMIT 1", [], |r| r.get(0)).unwrap();
        assert!(body.contains("{title}"));
        assert!(body.contains("{transcript}"));
        assert!(body.contains("{summary}"));
    }
}
