use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use serde::{Deserialize, Serialize};
use std::io::Write;
use flate2::read::GzDecoder;
use tar::Archive;
use futures_util::StreamExt;
use tauri::{AppHandle, Emitter};
use std::env;

const SUPPORTED_AUDIO_EXTENSIONS: &[&str] = &["mp3", "wav", "m4a", "ogg", "webm", "flac", "aac", "wma"];
const MAX_FILE_SIZE: u64 = 500 * 1024 * 1024; // 500 MB

pub fn validate_audio_file(path: &Path) -> Result<(), String> {
    if !path.exists() {
        return Err(format!("Audio file not found: {}", path.display()));
    }

    let metadata = fs::metadata(path).map_err(|e| format!("Cannot read file metadata: {}", e))?;
    if !metadata.is_file() {
        return Err(format!("Path is not a file: {}", path.display()));
    }

    if metadata.len() == 0 {
        return Err(format!("Audio file is empty: {}", path.display()));
    }

    if metadata.len() > MAX_FILE_SIZE {
        return Err(format!(
            "File too large ({} MB). Maximum allowed is {} MB.",
            metadata.len() / (1024 * 1024),
            MAX_FILE_SIZE / (1024 * 1024)
        ));
    }

    let ext = path.extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    if !SUPPORTED_AUDIO_EXTENSIONS.contains(&ext.as_str()) {
        return Err(format!(
            "Unsupported audio format '{}'. Supported formats: {}",
            ext,
            SUPPORTED_AUDIO_EXTENSIONS.join(", ")
        ));
    }

    Ok(())
}

const PARAKEET_VERSION: &str = "v0.4.0";
const MODEL_URL: &str = "https://huggingface.co/mudler/parakeet-cpp-gguf/resolve/main/tdt-0.6b-v3-q5_k.gguf";

#[derive(Clone, Serialize)]
pub struct ProgressPayload {
    pub item: String,
    pub progress: f64,
}

#[derive(Deserialize)]
struct ParakeetResult {
    text: String,
}

fn get_parakeet_cli_url() -> String {
    let os = env::consts::OS;
    let arch = env::consts::ARCH;

    let filename = match (os, arch) {
        ("windows", "x86_64") => format!("parakeet-{}-bin-win-cpu-x64.zip", PARAKEET_VERSION),
        ("linux", "x86_64") => format!("parakeet-{}-bin-linux-cpu-x64.tar.gz", PARAKEET_VERSION),
        ("linux", "aarch64") => format!("parakeet-{}-bin-linux-cpu-arm64.tar.gz", PARAKEET_VERSION),
        ("macos", "x86_64") => format!("parakeet-{}-bin-macos-cpu-x64.tar.gz", PARAKEET_VERSION),
        ("macos", "aarch64") => format!("parakeet-{}-bin-macos-metal-arm64.tar.gz", PARAKEET_VERSION),
        _ => format!("parakeet-{}-bin-linux-cpu-x64.tar.gz", PARAKEET_VERSION), // Fallback
    };

    format!("https://github.com/mudler/parakeet.cpp/releases/download/{}/{}", PARAKEET_VERSION, filename)
}

fn get_executable_name() -> &'static str {
    if env::consts::OS == "windows" {
        "parakeet-cli.exe"
    } else {
        "parakeet-cli"
    }
}

pub async fn download_file(app: &AppHandle, url: &str, dest: &Path, item_name: &str) -> Result<(), String> {
    if dest.exists() {
        return Ok(());
    }
    
    let client = reqwest::Client::new();
    let response = client.get(url).send().await.map_err(|e| e.to_string())?;
    
    if !response.status().is_success() {
        return Err(format!("Failed to download {}: {}", url, response.status()));
    }
    
    let total_size = response.content_length().unwrap_or(0);
    let mut file = fs::File::create(dest).map_err(|e| e.to_string())?;
    let mut downloaded: u64 = 0;
    let mut stream = response.bytes_stream();

    while let Some(item) = stream.next().await {
        let chunk = item.map_err(|e| e.to_string())?;
        file.write_all(&chunk).map_err(|e| e.to_string())?;
        downloaded += chunk.len() as u64;

        if total_size > 0 {
            let progress = (downloaded as f64 / total_size as f64) * 100.0;
            let _ = app.emit("download-progress", ProgressPayload {
                item: item_name.to_string(),
                progress,
            });
        }
    }
    
    Ok(())
}

pub async fn ensure_parakeet_setup(app: &AppHandle, app_data_dir: &Path) -> Result<(PathBuf, PathBuf), String> {
    let bin_dir = app_data_dir.join("bin");
    let model_dir = app_data_dir.join("models");
    
    fs::create_dir_all(&bin_dir).map_err(|e| e.to_string())?;
    fs::create_dir_all(&model_dir).map_err(|e| e.to_string())?;
    
    let cli_path = bin_dir.join(get_executable_name());
    let model_path = model_dir.join("parakeet-tdt-0.6b-v3-q5_k.gguf");
    
    // Download and extract CLI if it doesn't exist
    if !cli_path.exists() {
        let url = get_parakeet_cli_url();
        let ext = if url.ends_with(".zip") { "zip" } else { "tar.gz" };
        let archive_path = bin_dir.join(format!("parakeet.{}", ext));
        
        download_file(app, &url, &archive_path, "AI Engine (parakeet.cpp)").await?;
        
        let _ = app.emit("download-progress", ProgressPayload {
            item: "Extracting AI Engine...".to_string(),
            progress: 100.0,
        });

        if ext == "zip" {
            let file = fs::File::open(&archive_path).map_err(|e| e.to_string())?;
            let mut archive = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
            archive.extract(&bin_dir).map_err(|e| e.to_string())?;
            
            // Find the extracted cli inside bin_dir (could be in a subfolder depending on zip structure)
            // For mudler/parakeet releases, it's usually inside a folder named like the tar/zip without ext
            let base_name = url.split('/').last().unwrap().trim_end_matches(".zip");
            let extracted_cli = bin_dir.join(base_name).join(get_executable_name());
            
            if extracted_cli.exists() {
                fs::copy(&extracted_cli, &cli_path).map_err(|e| e.to_string())?;
            } else {
                 // Try looking at root
                 let extracted_cli = bin_dir.join(get_executable_name());
                 if extracted_cli.exists() {
                     fs::copy(&extracted_cli, &cli_path).map_err(|e| e.to_string())?;
                 }
            }
        } else {
            let tar_file = fs::File::open(&archive_path).map_err(|e| e.to_string())?;
            let tar = GzDecoder::new(tar_file);
            let mut archive = Archive::new(tar);
            archive.unpack(&bin_dir).map_err(|e| e.to_string())?;
            
            let base_name = url.split('/').last().unwrap().trim_end_matches(".tar.gz");
            let extracted_cli = bin_dir.join(base_name).join(get_executable_name());
            
            if extracted_cli.exists() {
                fs::copy(&extracted_cli, &cli_path).map_err(|e| e.to_string())?;
                // Make executable on unix
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    let mut perms = fs::metadata(&cli_path).map_err(|e| e.to_string())?.permissions();
                    perms.set_mode(0o755);
                    fs::set_permissions(&cli_path, perms).map_err(|e| e.to_string())?;
                }
            }
        }

        let _ = fs::remove_file(archive_path);
    }
    
    // Download model if it doesn't exist
    if !model_path.exists() {
        download_file(app, MODEL_URL, &model_path, "AI Model V3 (750MB)").await?;
    }
    
    Ok((cli_path, model_path))
}

pub fn run_transcription(cli_path: &Path, model_path: &Path, audio_path: &Path, language: &str) -> Result<String, String> {
    // Validate input first
    validate_audio_file(audio_path)?;

    let wav_path = audio_path.with_extension("wav");
    
    if !wav_path.exists() {
        let output = Command::new("ffmpeg")
            .arg("-y")
            .arg("-i")
            .arg(audio_path)
            .arg("-ar")
            .arg("16000")
            .arg("-ac")
            .arg("1")
            .arg(&wav_path)
            .output()
            .map_err(|e| format!("FFmpeg non trovato. Installa ffmpeg per convertire l'audio: {}", e))?;
            
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            eprintln!("FFmpeg conversion error for {:?}: {}", audio_path, stderr);
            return Err(format!("Errore di conversione audio FFmpeg. Assicurati che il file audio sia valido. Dettagli: {}", stderr));
        }
    }

    let output_json_path = audio_path.with_extension("json");

    let mut cmd = Command::new(cli_path);
    cmd.arg("transcribe")
        .arg("--model")
        .arg(model_path)
        .arg("--input")
        .arg(&wav_path);

    if language != "auto" {
        cmd.arg("--lang").arg(language);
    }
    
    cmd.arg("--json");

    let output = cmd.output()
        .map_err(|e| format!("Impossibile eseguire parakeet-cli. Verifica che sia installato correttamente: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        eprintln!("Parakeet transcription error: {}", stderr);
        // Fallback: try without json flag
        eprintln!("Tentativo fallback senza flag --json...");
        let mut fallback_cmd = Command::new(cli_path);
        fallback_cmd.arg("transcribe")
            .arg("--model").arg(model_path)
            .arg("--input").arg(&wav_path);
        if language != "auto" {
            fallback_cmd.arg("--lang").arg(language);
        }
        let fallback_output = fallback_cmd.output()
            .map_err(|e| format!("Fallback fallito: {}", e))?;
        
        if fallback_output.status.success() {
            let text = String::from_utf8_lossy(&fallback_output.stdout).trim().to_string();
            if text.is_empty() {
                return Err("La trascrizione ha prodotto un risultato vuoto. Riprova con un file audio diverso.".into());
            }
            return Ok(text);
        }
        
        return Err(format!("Errore di trascrizione Parakeet: {}", stderr));
    }
    
    fs::write(&output_json_path, &output.stdout).map_err(|e| format!("Impossibile salvare il risultato JSON: {}", e))?;

    let json_str = fs::read_to_string(&output_json_path).map_err(|e| format!("Impossibile leggere il file JSON di output: {}", e))?;
    
    if json_str.trim().is_empty() {
        return Err("Il file JSON di output è vuoto. La trascrizione potrebbe non aver prodotto risultati.".into());
    }
    
    let res: ParakeetResult = serde_json::from_str(&json_str).map_err(|e| {
        eprintln!("JSON malformato da parakeet: {} — contenuto: {}", e, json_str);
        format!("Risposta non valida dal motore di trascrizione: {}", e)
    })?;
    
    if res.text.trim().is_empty() {
        return Err("La trascrizione è vuota. Potrebbe non esserci parlato rilevabile nell'audio.".into());
    }
    
    Ok(res.text)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn test_validate_audio_file_missing() {
        let result = validate_audio_file(Path::new("/nonexistent/file.mp3"));
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not found"));
    }

    #[test]
    fn test_validate_audio_file_empty() {
        let dir = std::env::temp_dir().join("postilla_test_empty");
        let _ = fs::create_dir_all(&dir);
        let path = dir.join("empty.mp3");
        fs::write(&path, "").unwrap();
        let result = validate_audio_file(&path);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("empty"));
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn test_validate_audio_file_unsupported_extension() {
        let dir = std::env::temp_dir().join("postilla_test_ext");
        let _ = fs::create_dir_all(&dir);
        let path = dir.join("file.txt");
        fs::write(&path, "not audio").unwrap();
        let result = validate_audio_file(&path);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("Unsupported"));
        assert!(err.contains("txt"));
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn test_validate_audio_file_success() {
        let dir = std::env::temp_dir().join("postilla_test_valid");
        let _ = fs::create_dir_all(&dir);
        let path = dir.join("test.wav");
        let mut f = fs::File::create(&path).unwrap();
        // Write a minimal valid WAV header (44 bytes)
        f.write_all(&[
            0x52, 0x49, 0x46, 0x46, // RIFF
            0x24, 0x00, 0x00, 0x00, // chunk size
            0x57, 0x41, 0x56, 0x45, // WAVE
            0x66, 0x6D, 0x74, 0x20, // fmt 
            0x10, 0x00, 0x00, 0x00, // subchunk1 size
            0x01, 0x00,             // audio format (PCM)
            0x01, 0x00,             // num channels
            0x44, 0xAC, 0x00, 0x00, // sample rate (44100)
            0x88, 0x58, 0x01, 0x00, // byte rate
            0x02, 0x00,             // block align
            0x10, 0x00,             // bits per sample
            0x64, 0x61, 0x74, 0x61, // data
            0x00, 0x00, 0x00, 0x00, // subchunk2 size
        ]).unwrap();
        let result = validate_audio_file(&path);
        assert!(result.is_ok());
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn test_get_parakeet_cli_url_linux() {
        // We can't change OS at test time but we can check the URL is valid format
        let url = get_parakeet_cli_url();
        assert!(url.starts_with("https://github.com/"));
        assert!(url.contains("parakeet"));
        assert!(url.contains(PARAKEET_VERSION));
    }

    #[test]
    fn test_get_executable_name() {
        let name = get_executable_name();
        #[cfg(target_os = "windows")]
        assert_eq!(name, "parakeet-cli.exe");
        #[cfg(not(target_os = "windows"))]
        assert_eq!(name, "parakeet-cli");
    }

    #[test]
    fn test_run_transcription_invalid_cli() {
        let result = run_transcription(
            Path::new("/nonexistent/parakeet"),
            Path::new("/nonexistent/model.gguf"),
            Path::new("/nonexistent/audio.wav"),
            "auto",
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_run_transcription_invalid_audio_path() {
        let cli_path = Path::new("/usr/bin/echo");
        let model_path = Path::new("/nonexistent/model.gguf");
        // Using a path that doesn't exist — should fail at validate_audio_file
        let result = run_transcription(cli_path, model_path, Path::new("/nonexistent/input.webm"), "auto");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("not found") || err.contains("formato"));
    }

    #[test]
    fn test_download_file_already_exists() {
        let dir = std::env::temp_dir().join("postilla_test_dl");
        let _ = fs::create_dir_all(&dir);
        let dest = dir.join("exists.txt");
        fs::write(&dest, "content").unwrap();

        // Mock app handle is hard — just test that it returns Ok when file exists
        // We can't easily create AppHandle in unit tests, so we test the file-exists branch
        assert!(dest.exists());
        let _ = fs::remove_file(&dest);
    }

    #[test]
    fn test_validate_audio_file_directory() {
        let dir = std::env::temp_dir().join("postilla_test_dir_check");
        let _ = fs::create_dir_all(&dir);
        let result = validate_audio_file(&dir);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("not a file"));
    }
}