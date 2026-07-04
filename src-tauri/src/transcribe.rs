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
            .map_err(|e| format!("FFmpeg failed (make sure it's installed on your system to convert audio): {}", e))?;
            
        if !output.status.success() {
             return Err(format!("FFmpeg error: {:?}", String::from_utf8_lossy(&output.stderr)));
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

    // run parakeet, and redirect output to json file
    let output = cmd.output()
        .map_err(|e| format!("Failed to execute parakeet-cli: {}", e))?;

    if !output.status.success() {
        return Err(format!("Parakeet error: {}", String::from_utf8_lossy(&output.stderr)));
    }
    
    fs::write(&output_json_path, &output.stdout).map_err(|e| format!("Failed to write JSON output: {}", e))?;

    let json_str = fs::read_to_string(&output_json_path).map_err(|e| format!("Failed to read output JSON: {}", e))?;
    let res: ParakeetResult = serde_json::from_str(&json_str).map_err(|e| format!("Invalid JSON from parakeet: {}", e))?;
    
    Ok(res.text)
}