pub mod db;
pub mod transcribe;
pub mod llm;
pub mod license;
pub mod remote_llm;
pub mod speaker;

use db::{Session, Template, ExportTemplate, TranscriptBlock, Speaker, init_db};
use rusqlite::Connection;
use std::sync::Mutex;
use std::fs;
use std::path::PathBuf;
use tauri::{AppHandle, Manager, State, Emitter};
use chrono::Utc;
use machine_uid;
use serde::Serialize;

pub struct AppState {
    pub db: Mutex<Option<Connection>>,
    pub ollama_url: Mutex<String>,
}

#[tauri::command]
fn get_device_id() -> Result<String, String> {
    machine_uid::get().map_err(|e| format!("Impossibile ottenere l'ID del dispositivo: {}", e))
}

#[tauri::command]
async fn verify_license(license_key: String, device_id: String) -> Result<bool, String> {
    // Prima proviamo la chiave di test in locale, così non ti blocco lo sviluppo
    if license_key == "POSTILLA-PRO-123" {
        return Ok(true);
    }
    
    // Altrimenti chiamiamo l'API vera di Keygen.sh
    license::verify_license(&license_key, &device_id).await
}

#[tauri::command]
fn get_sessions(state: State<'_, AppState>) -> Result<Vec<Session>, String> {
    let db_guard = state.db.lock().unwrap();
    let conn = db_guard.as_ref().ok_or("Database not initialized")?;

    let mut stmt = conn
        .prepare("SELECT id, session_type, title, created_at, updated_at, status, file_path, transcript, summary, mind_map, template_id, participants, tags FROM sessions ORDER BY created_at DESC")
        .map_err(|e| e.to_string())?;

    let session_iter = stmt
        .query_map([], |row| {
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
        })
        .map_err(|e| e.to_string())?;

    let mut sessions = Vec::new();
    for session in session_iter {
        sessions.push(session.map_err(|e| e.to_string())?);
    }

    Ok(sessions)
}

#[tauri::command]
fn create_session(state: State<'_, AppState>, session_type: String, participants: Option<String>) -> Result<Session, String> {
    let db_guard = state.db.lock().unwrap();
    let conn = db_guard.as_ref().ok_or("Database not initialized")?;

    let now = Utc::now().to_rfc3339();
    let title = "Nuova sessione".to_string();
    let status = "pending".to_string();
    let participants = participants.unwrap_or_default();

    conn.execute(
        "INSERT INTO sessions (session_type, title, created_at, updated_at, status, participants) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        (&session_type, &title, &now, &now, &status, &participants),
    )
    .map_err(|e| e.to_string())?;

    let id = conn.last_insert_rowid();

    Ok(Session {
        id,
        session_type,
        title,
        created_at: now.clone(),
        updated_at: now,
        status,
        file_path: None,
        transcript: None,
        summary: None,
        mind_map: None,
        template_id: None,
        participants: Some(participants).filter(|p| !p.is_empty()),
        tags: None,
    })
}

#[tauri::command]
async fn import_audio(app: AppHandle, state: State<'_, AppState>, session_id: i64, source_path: String) -> Result<String, String> {
    // 1. Get media directory
    let app_data_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let media_dir = app_data_dir.join("media");
    if !media_dir.exists() {
        fs::create_dir_all(&media_dir).map_err(|e| e.to_string())?;
    }

    // 2. Determine extension and target path
    let source_path_buf = PathBuf::from(&source_path);
    let extension = source_path_buf.extension().and_then(|e| e.to_str()).unwrap_or("mp3");
    let file_name = format!("session_{}.{}", session_id, extension);
    let target_path = media_dir.join(&file_name);

    // 3. Copy file
    fs::copy(&source_path_buf, &target_path).map_err(|e| format!("Failed to copy file: {}", e))?;

    // 4. Update Database
    let target_path_str = target_path.to_string_lossy().to_string();
    let db_guard = state.db.lock().unwrap();
    let conn = db_guard.as_ref().ok_or("Database not initialized")?;
    
    conn.execute(
        "UPDATE sessions SET file_path = ?1, status = 'processing' WHERE id = ?2",
        (&target_path_str, &session_id),
    ).map_err(|e| e.to_string())?;

    Ok(target_path_str)
}

#[tauri::command]
async fn save_audio_recording(state: State<'_, AppState>, session_id: i64, target_path_str: String) -> Result<String, String> {
    // Note: the file is actually already saved by the frontend via Tauri FS plugin.
    // This command now just updates the database to point to it.
    
    let db_guard = state.db.lock().unwrap();
    let conn = db_guard.as_ref().ok_or("Database not initialized")?;
    
    conn.execute(
        "UPDATE sessions SET file_path = ?1, status = 'processing' WHERE id = ?2",
        (&target_path_str, &session_id),
    ).map_err(|e| e.to_string())?;

    Ok(target_path_str)
}

#[tauri::command]
async fn transcribe_session(app: AppHandle, state: State<'_, AppState>, session_id: i64, language: String) -> Result<String, String> {
    let app_data_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    
    // 1. Get file path from DB
    let file_path = {
        let db_guard = state.db.lock().unwrap();
        let conn = db_guard.as_ref().ok_or("Database not initialized")?;
        let mut stmt = conn.prepare("SELECT file_path FROM sessions WHERE id = ?1").unwrap();
        let path: Option<String> = stmt.query_row([&session_id], |row| row.get(0)).unwrap_or(None);
        path.ok_or("No file path associated with this session")?
    };

    let audio_path = PathBuf::from(file_path);

    // 2. Ensure parakeet-cli and model exist (now async to report progress)
    let (cli_path, model_path) = transcribe::ensure_parakeet_setup(&app, &app_data_dir).await?;

    let _ = app.emit("download-progress", transcribe::ProgressPayload {
        item: "Transcribing audio locally...".to_string(),
        progress: 100.0,
    });

    // 3. Run transcription
    let transcript = transcribe::run_transcription(&cli_path, &model_path, &audio_path, &language)?;

    // 4. Update DB
    {
        let db_guard = state.db.lock().unwrap();
        let conn = db_guard.as_ref().ok_or("Database not initialized")?;
        conn.execute(
            "UPDATE sessions SET transcript = ?1, status = 'completed' WHERE id = ?2",
            (&transcript, &session_id),
        ).map_err(|e| e.to_string())?;
        db::rebuild_fts(conn);
    }

    Ok(transcript)
}


#[tauri::command]
async fn get_ollama_models(state: State<'_, AppState>) -> Result<Vec<llm::EvaluatedModel>, String> {
    let url = state.ollama_url.lock().unwrap().clone();
    llm::get_available_models_at(&url).await
}

#[tauri::command]
async fn discover_ollama() -> Vec<llm::OllamaInstance> {
    llm::discover_ollama_instances().await
}

#[tauri::command]
fn set_ollama_url(state: State<'_, AppState>, url: String) {
    let mut stored = state.ollama_url.lock().unwrap();
    *stored = url;
}

#[tauri::command]
async fn summarize_session(state: State<'_, AppState>, session_id: i64, provider: String, api_key: String, model: String, template_id: Option<i64>) -> Result<String, String> {
    let (transcript, session_type) = {
        let db_guard = state.db.lock().unwrap();
        let conn = db_guard.as_ref().ok_or("Database not initialized")?;
        let mut stmt = conn.prepare("SELECT transcript, session_type FROM sessions WHERE id = ?1").unwrap();
        stmt.query_row([&session_id], |row| {
            let t: Option<String> = row.get(0).unwrap_or(None);
            let st: String = row.get(1).unwrap_or_default();
            Ok((t, st))
        }).map_err(|e| format!("Session not found: {}", e))?
    };

    let transcript = transcript.ok_or("No transcript found for this session to summarize.")?;

    // Resolve system prompt
    let system_prompt = if let Some(tid) = template_id {
        let db_guard = state.db.lock().unwrap();
        let conn = db_guard.as_ref().ok_or("Database not initialized")?;
        let prompt: Option<String> = conn.query_row(
            "SELECT system_prompt FROM templates WHERE id = ?1",
            [&tid],
            |row| row.get(0),
        ).ok();
        prompt
    } else {
        // Use default template for session_type
        let db_guard = state.db.lock().unwrap();
        let conn = db_guard.as_ref().ok_or("Database not initialized")?;
        let prompt: Option<String> = conn.query_row(
            "SELECT system_prompt FROM templates WHERE session_type = ?1 LIMIT 1",
            [&session_type],
            |row| row.get(0),
        ).ok();
        prompt
    };

    let ollama_url = state.ollama_url.lock().unwrap().clone();
    let summary = llm::dispatch_summary(&provider, &api_key, &model, &transcript, system_prompt.as_deref(), &ollama_url).await?;

    {
        let db_guard = state.db.lock().unwrap();
        let conn = db_guard.as_ref().ok_or("Database not initialized")?;
        conn.execute(
            "UPDATE sessions SET summary = ?1 WHERE id = ?2",
            (&summary, &session_id),
        ).map_err(|e| e.to_string())?;
        db::rebuild_fts(conn);
    }

    Ok(summary)
}

#[tauri::command]
async fn generate_session_title(state: State<'_, AppState>, session_id: i64, provider: String, api_key: String, model: String) -> Result<String, String> {
    let (transcript, session_type) = {
        let db_guard = state.db.lock().unwrap();
        let conn = db_guard.as_ref().ok_or("Database not initialized")?;
        let mut stmt = conn.prepare("SELECT transcript, session_type FROM sessions WHERE id = ?1").unwrap();
        stmt.query_row([&session_id], |row| {
            let t: Option<String> = row.get(0).unwrap_or(None);
            let st: String = row.get(1).unwrap_or_default();
            Ok((t, st))
        }).map_err(|e| format!("Session not found: {}", e))?
    };

    let transcript = transcript.ok_or("No transcript available. Transcribe the session first.")?;
    let ollama_url = state.ollama_url.lock().unwrap().clone();
    let title = llm::dispatch_title(&provider, &api_key, &model, &transcript, &session_type, &ollama_url).await?;

    {
        let db_guard = state.db.lock().unwrap();
        let conn = db_guard.as_ref().ok_or("Database not initialized")?;
        conn.execute(
            "UPDATE sessions SET title = ?1 WHERE id = ?2",
            (&title, &session_id),
        ).map_err(|e| e.to_string())?;
        db::rebuild_fts(conn);
    }

    Ok(title)
}

#[tauri::command]
async fn generate_mind_map(state: State<'_, AppState>, session_id: i64, provider: String, api_key: String, model: String) -> Result<String, String> {
    let (transcript, summary, session_type) = {
        let db_guard = state.db.lock().unwrap();
        let conn = db_guard.as_ref().ok_or("Database not initialized")?;
        let mut stmt = conn.prepare("SELECT transcript, summary, session_type FROM sessions WHERE id = ?1").unwrap();
        stmt.query_row([&session_id], |row| {
            let t: Option<String> = row.get(0).unwrap_or(None);
            let s: Option<String> = row.get(1).unwrap_or(None);
            let st: String = row.get(2).unwrap_or_default();
            Ok((t, s, st))
        }).map_err(|e| format!("Session not found: {}", e))?
    };

    let transcript = transcript.ok_or("No transcript available.")?;
    let ollama_url = state.ollama_url.lock().unwrap().clone();
    let mind_map = llm::dispatch_mind_map(&provider, &api_key, &model, &transcript, summary.as_deref(), &session_type, &ollama_url).await?;

    {
        let db_guard = state.db.lock().unwrap();
        let conn = db_guard.as_ref().ok_or("Database not initialized")?;
        conn.execute(
            "UPDATE sessions SET mind_map = ?1 WHERE id = ?2",
            (&mind_map, &session_id),
        ).map_err(|e| e.to_string())?;
    }

    Ok(mind_map)
}

#[tauri::command]
fn update_session_type(state: State<'_, AppState>, session_id: i64, session_type: String) -> Result<(), String> {
    let db_guard = state.db.lock().unwrap();
    let conn = db_guard.as_ref().ok_or("Database not initialized")?;
    let now = Utc::now().to_rfc3339();
    conn.execute(
        "UPDATE sessions SET session_type = ?1, updated_at = ?2 WHERE id = ?3",
        (&session_type, &now, &session_id),
    ).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
fn update_session_title(state: State<'_, AppState>, session_id: i64, title: String) -> Result<(), String> {
    let db_guard = state.db.lock().unwrap();
    let conn = db_guard.as_ref().ok_or("Database not initialized")?;
    let now = Utc::now().to_rfc3339();
    conn.execute(
        "UPDATE sessions SET title = ?1, updated_at = ?2 WHERE id = ?3",
        (&title, &now, &session_id),
    ).map_err(|e| e.to_string())?;
    db::rebuild_fts(conn);
    Ok(())
}

#[tauri::command]
fn update_session_transcript(state: State<'_, AppState>, session_id: i64, transcript: String) -> Result<(), String> {
    let db_guard = state.db.lock().unwrap();
    let conn = db_guard.as_ref().ok_or("Database not initialized")?;
    conn.execute(
        "UPDATE sessions SET transcript = ?1 WHERE id = ?2",
        (&transcript, &session_id),
    ).map_err(|e| e.to_string())?;
    db::rebuild_fts(conn);
    Ok(())
}

#[tauri::command]
fn update_session_summary(state: State<'_, AppState>, session_id: i64, summary: String) -> Result<(), String> {
    let db_guard = state.db.lock().unwrap();
    let conn = db_guard.as_ref().ok_or("Database not initialized")?;
    conn.execute(
        "UPDATE sessions SET summary = ?1 WHERE id = ?2",
        (&summary, &session_id),
    ).map_err(|e| e.to_string())?;
    db::rebuild_fts(conn);
    Ok(())
}

#[tauri::command]
fn get_templates(state: State<'_, AppState>) -> Result<Vec<Template>, String> {
    let db_guard = state.db.lock().unwrap();
    let conn = db_guard.as_ref().ok_or("Database not initialized")?;

    let mut stmt = conn
        .prepare("SELECT id, name, session_type, system_prompt FROM templates ORDER BY session_type, id")
        .map_err(|e| e.to_string())?;

    let rows = stmt
        .query_map([], |row| {
            Ok(Template {
                id: row.get(0)?,
                name: row.get(1)?,
                session_type: row.get(2)?,
                system_prompt: row.get(3)?,
            })
        })
        .map_err(|e| e.to_string())?;

    let mut templates = Vec::new();
    for row in rows {
        templates.push(row.map_err(|e| e.to_string())?);
    }
    Ok(templates)
}

#[tauri::command]
fn save_template(state: State<'_, AppState>, id: Option<i64>, name: String, session_type: String, system_prompt: String) -> Result<Template, String> {
    let db_guard = state.db.lock().unwrap();
    let conn = db_guard.as_ref().ok_or("Database not initialized")?;

    if let Some(tid) = id {
        conn.execute(
            "UPDATE templates SET name = ?1, session_type = ?2, system_prompt = ?3 WHERE id = ?4",
            (&name, &session_type, &system_prompt, &tid),
        ).map_err(|e| e.to_string())?;
        Ok(Template { id: tid, name, session_type, system_prompt })
    } else {
        conn.execute(
            "INSERT INTO templates (name, session_type, system_prompt) VALUES (?1, ?2, ?3)",
            (&name, &session_type, &system_prompt),
        ).map_err(|e| e.to_string())?;
        let new_id = conn.last_insert_rowid();
        Ok(Template { id: new_id, name, session_type, system_prompt })
    }
}

#[tauri::command]
fn delete_template(state: State<'_, AppState>, id: i64) -> Result<(), String> {
    let db_guard = state.db.lock().unwrap();
    let conn = db_guard.as_ref().ok_or("Database not initialized")?;
    conn.execute("DELETE FROM templates WHERE id = ?1", [&id])
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
fn update_session_participants(state: State<'_, AppState>, session_id: i64, participants: String) -> Result<(), String> {
    let db_guard = state.db.lock().unwrap();
    let conn = db_guard.as_ref().ok_or("Database not initialized")?;
    let now = Utc::now().to_rfc3339();
    conn.execute(
        "UPDATE sessions SET participants = ?1, updated_at = ?2 WHERE id = ?3",
        (&participants, &now, &session_id),
    ).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
fn update_session_tags(state: State<'_, AppState>, session_id: i64, tags: String) -> Result<(), String> {
    let db_guard = state.db.lock().unwrap();
    let conn = db_guard.as_ref().ok_or("Database not initialized")?;
    let now = Utc::now().to_rfc3339();
    conn.execute(
        "UPDATE sessions SET tags = ?1, updated_at = ?2 WHERE id = ?3",
        (&tags, &now, &session_id),
    ).map_err(|e| e.to_string())?;
    db::rebuild_fts(conn);
    Ok(())
}

// ── Speaker Intelligence Commands ──

#[tauri::command]
async fn run_diarization(app: AppHandle, state: State<'_, AppState>, session_id: i64, language: String) -> Result<Vec<TranscriptBlock>, String> {
    let app_data_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;

    // 1. Get file path from DB
    let file_path = {
        let db_guard = state.db.lock().unwrap();
        let conn = db_guard.as_ref().ok_or("Database not initialized")?;
        let mut stmt = conn.prepare("SELECT file_path FROM sessions WHERE id = ?1").unwrap();
        let path: Option<String> = stmt.query_row([&session_id], |row| row.get(0)).unwrap_or(None);
        path.ok_or("No file path associated with this session")?
    };

    let audio_path = PathBuf::from(&file_path);

    // 2. Run full transcription first
    let (cli_path, model_path) = transcribe::ensure_parakeet_setup(&app, &app_data_dir).await?;

    let _ = app.emit("download-progress", transcribe::ProgressPayload {
        item: "Transcribing audio...".to_string(),
        progress: 50.0,
    });

    let transcript = transcribe::run_transcription(&cli_path, &model_path, &audio_path, &language)?;

    let _ = app.emit("download-progress", transcribe::ProgressPayload {
        item: "Running speaker diarization...".to_string(),
        progress: 80.0,
    });

    // 3. Run diarization pipeline
    let db_guard = state.db.lock().unwrap();
    let conn = db_guard.as_ref().ok_or("Database not initialized")?;

    let blocks = speaker::SpeakerService::run_diarization_pipeline(
        conn,
        &app_data_dir,
        session_id,
        &file_path,
        &transcript,
    )?;

    drop(db_guard);

    Ok(blocks)
}

#[tauri::command]
fn get_transcript_blocks(state: State<'_, AppState>, session_id: i64) -> Result<Vec<TranscriptBlock>, String> {
    let db_guard = state.db.lock().unwrap();
    let conn = db_guard.as_ref().ok_or("Database not initialized")?;
    Ok(speaker::SpeakerRepo::get_blocks_for_session(conn, session_id))
}

#[tauri::command]
fn get_speaker_directory(state: State<'_, AppState>) -> Result<Vec<Speaker>, String> {
    let db_guard = state.db.lock().unwrap();
    let conn = db_guard.as_ref().ok_or("Database not initialized")?;
    Ok(speaker::SpeakerRepo::get_all_speakers(conn))
}

#[tauri::command]
fn rename_speaker(state: State<'_, AppState>, speaker_id: i64, new_name: String) -> Result<(), String> {
    let db_guard = state.db.lock().unwrap();
    let conn = db_guard.as_ref().ok_or("Database not initialized")?;
    speaker::SpeakerRepo::rename_speaker(conn, speaker_id, &new_name)
}

#[tauri::command]
fn delete_speaker(state: State<'_, AppState>, speaker_id: i64) -> Result<(), String> {
    let db_guard = state.db.lock().unwrap();
    let conn = db_guard.as_ref().ok_or("Database not initialized")?;
    speaker::SpeakerRepo::delete_speaker(conn, speaker_id)
}

#[tauri::command]
fn delete_transcript_blocks(state: State<'_, AppState>, session_id: i64) -> Result<(), String> {
    let db_guard = state.db.lock().unwrap();
    let conn = db_guard.as_ref().ok_or("Database not initialized")?;
    speaker::SpeakerRepo::delete_blocks_for_session(conn, session_id);
    Ok(())
}

// ── Export Templates ──

#[tauri::command]
fn get_export_templates(state: State<'_, AppState>) -> Result<Vec<ExportTemplate>, String> {
    let db_guard = state.db.lock().unwrap();
    let conn = db_guard.as_ref().ok_or("Database not initialized")?;
    let mut stmt = conn.prepare("SELECT id, name, body FROM export_templates ORDER BY id")
        .map_err(|e| e.to_string())?;
    let rows = stmt.query_map([], |row| {
        Ok(ExportTemplate { id: row.get(0)?, name: row.get(1)?, body: row.get(2)? })
    }).map_err(|e| e.to_string())?;
    let mut templates = Vec::new();
    for row in rows { templates.push(row.map_err(|e| e.to_string())?); }
    Ok(templates)
}

#[tauri::command]
fn save_export_template(state: State<'_, AppState>, id: Option<i64>, name: String, body: String) -> Result<ExportTemplate, String> {
    let db_guard = state.db.lock().unwrap();
    let conn = db_guard.as_ref().ok_or("Database not initialized")?;
    if let Some(tid) = id {
        conn.execute("UPDATE export_templates SET name = ?1, body = ?2 WHERE id = ?3", (&name, &body, &tid))
            .map_err(|e| e.to_string())?;
        Ok(ExportTemplate { id: tid, name, body })
    } else {
        conn.execute("INSERT INTO export_templates (name, body) VALUES (?1, ?2)", (&name, &body))
            .map_err(|e| e.to_string())?;
        Ok(ExportTemplate { id: conn.last_insert_rowid(), name, body })
    }
}

#[tauri::command]
fn delete_export_template(state: State<'_, AppState>, id: i64) -> Result<(), String> {
    let db_guard = state.db.lock().unwrap();
    let conn = db_guard.as_ref().ok_or("Database not initialized")?;
    conn.execute("DELETE FROM export_templates WHERE id = ?1", [&id])
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
fn export_session_with_template(state: State<'_, AppState>, session_id: i64, template_id: i64) -> Result<String, String> {
    let db_guard = state.db.lock().unwrap();
    let conn = db_guard.as_ref().ok_or("Database not initialized")?;

    let session = conn.query_row(
        "SELECT id, session_type, title, created_at, updated_at, status, file_path, transcript, summary, mind_map, template_id, participants, tags FROM sessions WHERE id = ?1",
        [&session_id],
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
    ).map_err(|e| format!("Session not found: {}", e))?;

    let export_tmpl = conn.query_row(
        "SELECT id, name, body FROM export_templates WHERE id = ?1",
        [&template_id],
        |row| Ok(ExportTemplate { id: row.get(0)?, name: row.get(1)?, body: row.get(2)? }),
    ).map_err(|e| format!("Export template not found: {}", e))?;

    let type_label = match session.session_type.as_str() {
        "meeting" => "Meeting", "voice_note" => "Voice Note",
        "lecture" => "Lecture", "import" => "Imported File", _ => &session.session_type,
    };

    let mut out = export_tmpl.body;
    let replacements: [(&str, &str); 8] = [
        ("title", &session.title),
        ("type", type_label),
        ("date", &session.created_at),
        ("participants", session.participants.as_deref().unwrap_or("")),
        ("tags", session.tags.as_deref().unwrap_or("")),
        ("transcript", session.transcript.as_deref().unwrap_or("")),
        ("summary", session.summary.as_deref().unwrap_or("")),
        ("mind_map", session.mind_map.as_deref().unwrap_or("")),
    ];
    for (key, val) in &replacements {
        out = out.replace(&format!("{{{}}}", key), val);
    }
    Ok(out)
}

#[tauri::command]
fn delete_session(state: State<'_, AppState>, session_id: i64) -> Result<(), String> {
    let db_guard = state.db.lock().unwrap();
    let conn = db_guard.as_ref().ok_or("Database not initialized")?;

    // Get file path before deleting
    let file_path: Option<String> = conn.query_row(
        "SELECT file_path FROM sessions WHERE id = ?1", [&session_id],
        |row| Ok(row.get(0).ok()),
    ).unwrap_or(None).flatten();

    conn.execute("DELETE FROM sessions WHERE id = ?1", [&session_id])
        .map_err(|e| e.to_string())?;
    db::rebuild_fts(conn);

    // Delete audio file if exists
    if let Some(path) = file_path {
        let _ = fs::remove_file(&path);
    }

    Ok(())
}

#[tauri::command]
fn export_session_srt(state: State<'_, AppState>, session_id: i64) -> Result<String, String> {
    let db_guard = state.db.lock().unwrap();
    let conn = db_guard.as_ref().ok_or("Database not initialized")?;

    let session: Session = conn.query_row(
        "SELECT id, session_type, title, created_at, updated_at, status, file_path, transcript, summary, mind_map, template_id, participants, tags FROM sessions WHERE id = ?1",
        [&session_id],
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
    ).map_err(|e| format!("Session not found: {}", e))?;

    let transcript = session.transcript.ok_or("No transcript available")?;

    let lines: Vec<&str> = transcript.lines().filter(|l| !l.trim().is_empty()).collect();
    let mut srt = String::new();
    let mut counter = 1;
    for chunk in lines.chunks(2) {
        let text = chunk.join(" ");
        let start_secs = (counter - 1) * 5;
        let end_secs = start_secs + 5;
        let start = format!("{:02}:{:02}:{:02},000", start_secs / 3600, (start_secs % 3600) / 60, start_secs % 60);
        let end = format!("{:02}:{:02}:{:02},000", end_secs / 3600, (end_secs % 3600) / 60, end_secs % 60);
        srt.push_str(&format!("{}\n{} --> {}\n{}\n\n", counter, start, end, text));
        counter += 1;
    }
    Ok(srt)
}

#[tauri::command]
fn export_session_vtt(state: State<'_, AppState>, session_id: i64) -> Result<String, String> {
    let srt = export_session_srt(state, session_id)?;
    let vtt = "WEBVTT\n\n".to_string() + srt.replace(",", ".").as_str();
    Ok(vtt)
}

#[tauri::command]
fn export_session_txt(state: State<'_, AppState>, session_id: i64) -> Result<String, String> {
    let db_guard = state.db.lock().unwrap();
    let conn = db_guard.as_ref().ok_or("Database not initialized")?;

    let session: Session = conn.query_row(
        "SELECT id, session_type, title, created_at, updated_at, status, file_path, transcript, summary, mind_map, template_id, participants, tags FROM sessions WHERE id = ?1",
        [&session_id],
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
    ).map_err(|e| format!("Session not found: {}", e))?;

    let mut txt = String::new();
    txt.push_str(&format!("{}\n", session.title));
    txt.push_str(&format!("{}\n", "=".repeat(session.title.len())));
    txt.push_str(&format!("Type: {}\n", session.session_type));
    txt.push_str(&format!("Date: {}\n", session.created_at));
    if let Some(ref p) = session.participants { if !p.is_empty() { txt.push_str(&format!("Participants: {}\n", p)); } }
    if let Some(ref t) = session.tags { if !t.is_empty() { txt.push_str(&format!("Tags: {}\n", t)); } }
    txt.push_str("\n---\n\n");
    if let Some(ref t) = session.transcript { txt.push_str(&format!("TRANSCRIPT\n{}\n\n---\n\n", t)); }
    if let Some(ref s) = session.summary { txt.push_str(&format!("SUMMARY\n{}\n\n---\n\n", s)); }
    if let Some(ref m) = session.mind_map { txt.push_str(&format!("MIND MAP\n{}\n\n", m)); }
    txt.push_str("---\nExported from Postilla\n");
    Ok(txt)
}

#[tauri::command]
fn export_session_obsidian(state: State<'_, AppState>, session_id: i64) -> Result<String, String> {
    let db_guard = state.db.lock().unwrap();
    let conn = db_guard.as_ref().ok_or("Database not initialized")?;

    let session: Session = conn.query_row(
        "SELECT id, session_type, title, created_at, updated_at, status, file_path, transcript, summary, mind_map, template_id, participants, tags FROM sessions WHERE id = ?1",
        [&session_id],
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
    ).map_err(|e| format!("Session not found: {}", e))?;

    let type_label = match session.session_type.as_str() {
        "meeting" => "Meeting", "voice_note" => "Voice Note",
        "lecture" => "Lecture", "import" => "Imported File", _ => &session.session_type,
    };

    let mut md = String::new();
    md.push_str("---\n");
    md.push_str(&format!("title: {}\n", session.title));
    md.push_str(&format!("type: {}\n", type_label));
    md.push_str(&format!("date: {}\n", session.created_at));
    if let Some(ref p) = session.participants { if !p.is_empty() { md.push_str(&format!("participants: {}\n", p)); } }
    if let Some(ref t) = session.tags { if !t.is_empty() { md.push_str(&format!("tags: [{}]\n", t.split(',').map(|s| format!("\"{}\"", s.trim())).collect::<Vec<_>>().join(", "))); } }
    md.push_str("---\n\n");

    if let Some(ref s) = session.summary { md.push_str(&format!("## Summary\n\n{}\n\n", s)); }
    if let Some(ref t) = session.transcript { md.push_str(&format!("## Transcript\n\n{}\n\n", t)); }
    if let Some(ref m) = session.mind_map { md.push_str(&format!("## Mind Map\n\n{}\n\n", m)); }
    md.push_str("*Exported from Postilla*\n");
    Ok(md)
}

#[tauri::command]
fn get_dashboard_stats(state: State<'_, AppState>) -> Result<serde_json::Value, String> {
    let db_guard = state.db.lock().unwrap();
    let conn = db_guard.as_ref().ok_or("Database not initialized")?;

    let total: i64 = conn.query_row("SELECT COUNT(*) FROM sessions", [], |r| r.get(0)).unwrap_or(0);
    let with_transcript: i64 = conn.query_row("SELECT COUNT(*) FROM sessions WHERE transcript IS NOT NULL AND transcript != ''", [], |r| r.get(0)).unwrap_or(0);
    let with_summary: i64 = conn.query_row("SELECT COUNT(*) FROM sessions WHERE summary IS NOT NULL AND summary != ''", [], |r| r.get(0)).unwrap_or(0);
    let total_audio_duration: f64 = total as f64 * 1800.0; // placeholder — real calc needs ffprobe per file
    let meetings: i64 = conn.query_row("SELECT COUNT(*) FROM sessions WHERE session_type = 'meeting'", [], |r| r.get(0)).unwrap_or(0);
    let voice_notes: i64 = conn.query_row("SELECT COUNT(*) FROM sessions WHERE session_type = 'voice_note'", [], |r| r.get(0)).unwrap_or(0);
    let lectures: i64 = conn.query_row("SELECT COUNT(*) FROM sessions WHERE session_type = 'lecture'", [], |r| r.get(0)).unwrap_or(0);
    let imports: i64 = conn.query_row("SELECT COUNT(*) FROM sessions WHERE session_type = 'import'", [], |r| r.get(0)).unwrap_or(0);

    Ok(serde_json::json!({
        "total": total,
        "with_transcript": with_transcript,
        "with_summary": with_summary,
        "total_audio_minutes": (total_audio_duration / 60.0).round() as i64,
        "by_type": {
            "meeting": meetings,
            "voice_note": voice_notes,
            "lecture": lectures,
            "import": imports,
        }
    }))
}

#[tauri::command]
fn cleanup_old_sessions(state: State<'_, AppState>, days: i64) -> Result<i64, String> {
    let db_guard = state.db.lock().unwrap();
    let conn = db_guard.as_ref().ok_or("Database not initialized")?;

    let cutoff = chrono::Utc::now() - chrono::Duration::days(days);
    let cutoff_str = cutoff.to_rfc3339();

    let mut stmt = conn.prepare("SELECT id, file_path FROM sessions WHERE updated_at < ?1").map_err(|e| e.to_string())?;
    let to_delete: Vec<(i64, Option<String>)> = stmt.query_map([&cutoff_str], |row| {
        Ok((row.get(0)?, row.get(1).unwrap_or(None)))
    }).map_err(|e| e.to_string())?.filter_map(|r| r.ok()).collect();

    let count = to_delete.len() as i64;
    for (id, path) in &to_delete {
        let _ = conn.execute("DELETE FROM sessions WHERE id = ?1", [id]);
        if let Some(p) = path { let _ = fs::remove_file(p); }
    }
    db::rebuild_fts(conn);
    Ok(count)
}

#[tauri::command]
fn get_help_topics() -> Vec<serde_json::Value> {
    vec![
        serde_json::json!({"title": "Registrare una sessione", "content": "Clicca su 'Nuova Registrazione' per avviare una registrazione. Autorizza il microfono quando richiesto. Al termine, clicca sul pulsante stop."}),
        serde_json::json!({"title": "Importare un file audio", "content": "Clicca su 'Importa Audio' e seleziona un file dal tuo computer. Formati supportati: MP3, WAV, M4A, OGG, WebM."}),
        serde_json::json!({"title": "Trascrivere audio", "content": "Dopo aver registrato o importato, clicca su 'Trascrivi'. Il modello Parakeet verrà scaricato automaticamente al primo utilizzo."}),
        serde_json::json!({"title": "Riassumere con AI", "content": "Configura un provider AI in Impostazioni (Ollama locale, OpenAI o Anthropic), poi clicca 'Riassumi'. Scegli tra template predefiniti per tipo di sessione."}),
        serde_json::json!({"title": "Esportare una sessione", "content": "Usa il pulsante 'Esporta' per scaricare in Markdown, TXT, SRT, VTT, o formato Obsidian. Crea template personalizzati in Impostazioni."}),
        serde_json::json!({"title": "Cercare tra le sessioni", "content": "Usa la barra di ricerca nella sidebar. La ricerca full-text trova risultati in titoli, trascrizioni, riassunti e tag."}),
        serde_json::json!({"title": "Provider AI disponibili", "content": "Ollama: locale e gratuito. OpenAI: ChatGPT. Anthropic: Claude. Configura le chiavi API in Impostazioni > AI Providers."}),
    ]
}

#[tauri::command]
fn get_audio_duration(path: String) -> Result<f64, String> {
    use std::process::Command;
    let output = Command::new("ffprobe")
        .args(["-v", "quiet", "-show_entries", "format=duration", "-of", "default=noprint_wrappers=1:nokey=1", &path])
        .output()
        .map_err(|e| format!("ffprobe not found: {}", e))?;
    if !output.status.success() {
        return Err("ffprobe failed to get duration".into());
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    stdout.trim().parse::<f64>().map_err(|_| "Could not parse duration".into())
}

#[tauri::command]
async fn rag_query(state: State<'_, AppState>, question: String, provider: String, api_key: String, model: String) -> Result<String, String> {
    // Search relevant transcripts
    let (results, ollama_url) = {
        let db_guard = state.db.lock().unwrap();
        let conn = db_guard.as_ref().ok_or("Database not initialized")?;
        let results = db::search_sessions(conn, &question);
        let ollama_url = state.ollama_url.lock().unwrap().clone();
        (results, ollama_url)
    };

    if results.is_empty() {
        return Err("No relevant sessions found. Try a different question.".into());
    }

    // Build context from top results
    let mut context = String::new();
    for (i, session) in results.iter().take(5).enumerate() {
        if let Some(ref t) = session.transcript {
            context.push_str(&format!("--- Sessione {}: {} ---\n{}\n\n", i + 1, session.title, t));
        }
    }

    llm::dispatch_rag_query(&provider, &api_key, &model, &question, &context, &ollama_url).await
}

#[tauri::command]
fn backup_database(app: AppHandle, dest_path: String) -> Result<(), String> {
    let app_data_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let db_path = app_data_dir.join("postilla.db");
    fs::copy(&db_path, &dest_path).map_err(|e| format!("Failed to backup database: {}", e))?;
    Ok(())
}

#[tauri::command]
fn restore_database(app: AppHandle, state: State<'_, AppState>, source_path: String) -> Result<(), String> {
    let data = fs::read(&source_path).map_err(|e| format!("Cannot read backup file: {}", e))?;
    if data.len() < 16 || &data[0..16] != b"SQLite format 3\0" {
        return Err("Not a valid SQLite database file".into());
    }

    let app_data_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let db_path = app_data_dir.join("postilla.db");

    let mut db_guard = state.db.lock().unwrap();
    *db_guard = None;
    fs::copy(&source_path, &db_path).map_err(|e| format!("Failed to restore database: {}", e))?;
    let conn = init_db(&app_data_dir).map_err(|e| format!("Failed to re-initialize database: {}", e))?;
    *db_guard = Some(conn);

    Ok(())
}

#[tauri::command]
async fn annotate_speakers(state: State<'_, AppState>, session_id: i64, provider: String, api_key: String, model: String) -> Result<String, String> {
    let (transcript, participants) = {
        let db_guard = state.db.lock().unwrap();
        let conn = db_guard.as_ref().ok_or("Database not initialized")?;
        let mut stmt = conn.prepare("SELECT transcript, participants FROM sessions WHERE id = ?1").unwrap();
        stmt.query_row([&session_id], |row| {
            let t: Option<String> = row.get(0).unwrap_or(None);
            let p: Option<String> = row.get(1).unwrap_or(None);
            Ok((t, p))
        }).map_err(|e| format!("Session not found: {}", e))?
    };

    let transcript = transcript.ok_or("No transcript found. Transcribe the session first.")?;
    let participants = participants.unwrap_or_default();
    if participants.trim().is_empty() {
        return Err("No participants set. Add participants first.".into());
    }

    let ollama_url = state.ollama_url.lock().unwrap().clone();
    let annotated = llm::dispatch_annotate_speakers(&provider, &api_key, &model, &transcript, &participants, &ollama_url).await?;

    // Save annotated transcript back
    {
        let db_guard = state.db.lock().unwrap();
        let conn = db_guard.as_ref().ok_or("Database not initialized")?;
        conn.execute(
            "UPDATE sessions SET transcript = ?1 WHERE id = ?2",
            (&annotated, &session_id),
        ).map_err(|e| e.to_string())?;
        db::rebuild_fts(conn);
    }

    Ok(annotated)
}

#[derive(Serialize)]
struct AudioData {
    mime: String,
    b64: String,
}

#[tauri::command]
fn read_audio_file(path: String) -> Result<AudioData, String> {
    use base64::Engine;
    let data = fs::read(&path).map_err(|e| format!("Failed to read audio file: {}", e))?;

    let mime = if data.len() > 4 {
        let header: [u8; 4] = [data[0], data[1], data[2], data[3]];
        match header {
            [0x52, 0x49, 0x46, 0x46] => "audio/wav",
            [0x4F, 0x67, 0x67, 0x53] => "audio/ogg",
            [0x66, 0x4C, 0x61, 0x43] => "audio/flac",
            [0x49, 0x44, 0x33, _]    => "audio/mpeg",
            [0xFF, 0xFB, _, _]       => "audio/mpeg",
            [0x00, 0x00, 0x00, 0x20] => "audio/mp4",
            [0x1A, 0x45, 0xDF, 0xA3] => "audio/webm",
            _ => {
                let ext = path.rsplit('.').next().unwrap_or("webm").to_lowercase();
                match ext.as_str() {
                    "ogg" => "audio/ogg",
                    "m4a" | "mp4" => "audio/mp4",
                    "wav" => "audio/wav",
                    "mp3" => "audio/mpeg",
                    _ => "audio/webm",
                }
            }
        }
    } else {
        "audio/webm"
    };

    Ok(AudioData { mime: mime.to_string(), b64: base64::engine::general_purpose::STANDARD.encode(&data) })
}

#[tauri::command]
fn search_sessions(state: State<'_, AppState>, query: String) -> Result<Vec<Session>, String> {
    let db_guard = state.db.lock().unwrap();
    let conn = db_guard.as_ref().ok_or("Database not initialized")?;
    Ok(db::search_sessions(conn, &query))
}

#[tauri::command]
fn export_session_markdown(state: State<'_, AppState>, session_id: i64) -> Result<String, String> {
    let db_guard = state.db.lock().unwrap();
    let conn = db_guard.as_ref().ok_or("Database not initialized")?;

    let session: Session = conn.query_row(
        "SELECT id, session_type, title, created_at, updated_at, status, file_path, transcript, summary, mind_map, template_id, participants, tags FROM sessions WHERE id = ?1",
        [&session_id],
        |row| {
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
        },
    ).map_err(|e| format!("Session not found: {}", e))?;

    let type_label = match session.session_type.as_str() {
        "meeting" => "Meeting",
        "voice_note" => "Voice Note",
        "lecture" => "Lecture",
        "import" => "Imported File",
        _ => &session.session_type,
    };

    let mut md = String::new();
    md.push_str(&format!("# {}\n\n", session.title));
    md.push_str(&format!("- **Type:** {}\n", type_label));
    md.push_str(&format!("- **Date:** {}\n", session.created_at));
    if let Some(ref p) = session.participants {
        if !p.is_empty() {
            md.push_str(&format!("- **Participants:** {}\n", p));
        }
    }
    if let Some(ref t) = session.tags {
        if !t.is_empty() {
            md.push_str(&format!("- **Tags:** {}\n", t));
        }
    }
    md.push_str("\n---\n\n");

    if let Some(ref t) = session.transcript {
        md.push_str("## Transcript\n\n");
        // Strip ** markers for clean markdown — keep as bold
        let clean = t.replace("**", "**");
        md.push_str(&clean);
        md.push_str("\n\n---\n\n");
    }

    if let Some(ref s) = session.summary {
        md.push_str("## Summary\n\n");
        md.push_str(s);
        md.push_str("\n\n---\n\n");
    }

    if let Some(ref m) = session.mind_map {
        md.push_str("## Mind Map\n\n");
        md.push_str(m);
        md.push_str("\n\n");
    }

    md.push_str("\n---\n*Exported from Postilla*\n");
    Ok(md)
}

#[tauri::command]
async fn get_remote_models(provider: String, api_key: String) -> Result<Vec<String>, String> {
    match provider.as_str() {
        "openai" => remote_llm::openai_list_models(&api_key).await,
        "anthropic" => remote_llm::anthropic_list_models(&api_key).await,
        _ => Err(format!("Unknown provider: {}", provider)),
    }
}

#[tauri::command]
async fn validate_api_key(provider: String, api_key: String) -> Result<bool, String> {
    match provider.as_str() {
        "openai" => {
            match remote_llm::openai_list_models(&api_key).await {
                Ok(_) => Ok(true),
                Err(_e) => Ok(false), // Don't leak error details
            }
        }
        "anthropic" => {
            match remote_llm::anthropic_list_models(&api_key).await {
                Ok(_) => Ok(true),
                Err(_) => Ok(false),
            }
        }
        _ => Err(format!("Unknown provider: {}", provider)),
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_os::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let app_data_dir = app.path().app_data_dir().expect("Failed to get app data dir");
            let conn = init_db(&app_data_dir).expect("Failed to initialize database");
            
            app.manage(AppState {
                db: Mutex::new(Some(conn)),
                ollama_url: Mutex::new("http://localhost:11434".to_string()),
            });

            #[cfg(target_os = "linux")]
            {
                use webkit2gtk::{SettingsExt, WebViewExt, PermissionRequestExt};
                if let Some(window) = app.get_webview_window("main") {
                    window.with_webview(move |webview| {
                        let inner = webview.inner();
                        if let Some(settings) = inner.settings() {
                            settings.set_enable_media_stream(true);
                        }
                        
                        inner.connect_permission_request(move |_webview, request| {
                            request.allow();
                            true
                        });
                    }).expect("Failed to setup webview");
                }
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_sessions, create_session, import_audio, save_audio_recording,
            transcribe_session, summarize_session, get_ollama_models,
            get_device_id, verify_license,
            update_session_type, update_session_title,
            generate_session_title, generate_mind_map,
            get_templates, save_template, delete_template,
            read_audio_file, get_remote_models, validate_api_key,
            discover_ollama, set_ollama_url, update_session_participants, annotate_speakers,
            export_session_markdown, search_sessions,
            update_session_transcript, update_session_summary, update_session_tags,
            backup_database, restore_database,
            get_export_templates, save_export_template, delete_export_template, export_session_with_template,
            rag_query, delete_session, get_audio_duration,
            export_session_srt, export_session_vtt, export_session_txt, export_session_obsidian,
            get_dashboard_stats, cleanup_old_sessions, get_help_topics,
            run_diarization, get_transcript_blocks, get_speaker_directory,
            rename_speaker, delete_speaker, delete_transcript_blocks,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}