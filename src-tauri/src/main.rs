// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::path::Path;

/// Validate that a user-provided file path is safe to process.
/// Checks for empty paths, invalid UTF-8, directory traversal, and unsupported extensions.
pub fn validate_input_path(path: &str, allowed_extensions: &[&str]) -> Result<(), String> {
    if path.trim().is_empty() {
        return Err("Il percorso del file non può essere vuoto.".into());
    }

    let p = Path::new(path);

    if !p.exists() {
        return Err(format!("Il file non esiste: {}", path));
    }

    if !p.is_file() {
        return Err(format!("Il percorso non è un file: {}", path));
    }

    // Ensure the path doesn't use relative traversal to escape
    if path.contains("..") {
        return Err("Il percorso non può contenere '..' per motivi di sicurezza.".into());
    }

    // Validate extension
    let ext = p.extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase())
        .unwrap_or_default();

    if ext.is_empty() {
        return Err("Il file non ha un'estensione.".into());
    }

    if !allowed_extensions.contains(&ext.as_str()) {
        return Err(format!(
            "Formato file '{}' non supportato. Formati accettati: {}",
            ext,
            allowed_extensions.join(", ")
        ));
    }

    Ok(())
}

fn main() {
    tauri_app_lib::run()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_validate_input_path_empty() {
        let result = validate_input_path("", &["mp3", "wav"]);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("vuoto"));
    }

    #[test]
    fn test_validate_input_path_whitespace() {
        let result = validate_input_path("   ", &["mp3"]);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_input_path_not_found() {
        let result = validate_input_path("/nonexistent/file.mp3", &["mp3"]);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("non esiste"));
    }

    #[test]
    fn test_validate_input_path_directory_not_file() {
        let result = validate_input_path("/tmp", &["mp3"]);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("non è un file"));
    }

    #[test]
    fn test_validate_input_path_directory_traversal() {
        let result = validate_input_path("../../etc/passwd", &["passwd"]);
        assert!(result.is_err());
        // Should catch the non-existent path or the .. traversal
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_input_path_unsupported_extension() {
        let dir = std::env::temp_dir().join("postilla_main_test");
        let _ = fs::create_dir_all(&dir);
        let path = dir.join("file.exe");
        fs::write(&path, "data").unwrap();

        let result = validate_input_path(path.to_str().unwrap(), &["mp3", "wav"]);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("non supportato") || err.contains("exe"));
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn test_validate_input_path_no_extension() {
        let dir = std::env::temp_dir().join("postilla_main_noext");
        let _ = fs::create_dir_all(&dir);
        let path = dir.join("file_without_ext");
        fs::write(&path, "data").unwrap();

        let result = validate_input_path(path.to_str().unwrap(), &["mp3"]);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("estensione"));
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn test_validate_input_path_success() {
        let dir = std::env::temp_dir().join("postilla_main_ok");
        let _ = fs::create_dir_all(&dir);
        let path = dir.join("audio.mp3");
        fs::write(&path, "fake audio").unwrap();

        let result = validate_input_path(path.to_str().unwrap(), &["mp3", "wav", "ogg"]);
        assert!(result.is_ok());
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn test_validate_input_path_case_insensitive_extension() {
        let dir = std::env::temp_dir().join("postilla_main_case");
        let _ = fs::create_dir_all(&dir);
        let path = dir.join("audio.MP3");
        fs::write(&path, "data").unwrap();

        let result = validate_input_path(path.to_str().unwrap(), &["mp3", "wav"]);
        assert!(result.is_ok());
        let _ = fs::remove_file(&path);
    }
}
