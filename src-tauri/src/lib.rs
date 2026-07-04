pub mod db;
pub mod transcribe;
pub mod llm;
pub mod license;

use db::{Session, init_db};
use rusqlite::Connection;
use std::sync::Mutex;
use std::fs;
use std::path::PathBuf;
use tauri::{AppHandle, Manager, State, Emitter};
use chrono::Utc;
use machine_uid;

pub struct AppState {
    pub db: Mutex<Option<Connection>>,
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
        .prepare("SELECT id, session_type, title, created_at, updated_at, status, file_path, transcript, summary FROM sessions ORDER BY created_at DESC")
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
fn create_session(state: State<'_, AppState>, session_type: String, title: String) -> Result<Session, String> {
    let db_guard = state.db.lock().unwrap();
    let conn = db_guard.as_ref().ok_or("Database not initialized")?;

    let now = Utc::now().to_rfc3339();
    let status = "pending".to_string();

    conn.execute(
        "INSERT INTO sessions (session_type, title, created_at, updated_at, status) VALUES (?1, ?2, ?3, ?4, ?5)",
        (&session_type, &title, &now, &now, &status),
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
    }

    Ok(transcript)
}


#[tauri::command]
async fn get_ollama_models() -> Result<Vec<llm::EvaluatedModel>, String> {
    llm::get_available_models().await
}

#[tauri::command]
async fn summarize_session(state: State<'_, AppState>, session_id: i64, model: String) -> Result<String, String> {
    // 1. Get transcript from DB
    let transcript = {
        let db_guard = state.db.lock().unwrap();
        let conn = db_guard.as_ref().ok_or("Database not initialized")?;
        let mut stmt = conn.prepare("SELECT transcript FROM sessions WHERE id = ?1").unwrap();
        let text: Option<String> = stmt.query_row([&session_id], |row| row.get(0)).unwrap_or(None);
        text.ok_or("No transcript found for this session to summarize.")?
    };

    // 2. Call Ollama local LLM
    let summary = llm::generate_summary(&transcript, &model).await?;

    // 3. Update DB
    {
        let db_guard = state.db.lock().unwrap();
        let conn = db_guard.as_ref().ok_or("Database not initialized")?;
        conn.execute(
            "UPDATE sessions SET summary = ?1 WHERE id = ?2",
            (&summary, &session_id),
        ).map_err(|e| e.to_string())?;
    }

    Ok(summary)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_os::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let app_data_dir = app.path().app_data_dir().expect("Failed to get app data dir");
            let conn = init_db(&app_data_dir).expect("Failed to initialize database");
            
            app.manage(AppState {
                db: Mutex::new(Some(conn)),
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
        .invoke_handler(tauri::generate_handler![get_sessions, create_session, import_audio, save_audio_recording, transcribe_session, summarize_session, get_ollama_models, get_device_id, verify_license])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}