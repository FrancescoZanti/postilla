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
