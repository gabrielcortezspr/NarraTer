use base64::Engine;
use serde::Serialize;
use std::path::PathBuf;

// Huge directories (node_modules…) get truncated; the tree is lazy per
// directory, so this only limits the current level.
const MAX_ENTRIES: usize = 2000;
const MAX_FILE_BYTES: u64 = 10 * 1024 * 1024;

#[derive(Debug, Serialize, Clone)]
pub struct FsEntry {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
}

#[derive(Debug, Serialize, Clone)]
pub struct FileBlob {
    pub base64: String,
    pub mime: String,
    pub size: u64,
}

fn expand_tilde(path: &str) -> PathBuf {
    if path == "~" {
        return dirs::home_dir().unwrap_or_else(|| PathBuf::from("/"));
    }
    if let Some(rest) = path.strip_prefix("~/") {
        return dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("/"))
            .join(rest);
    }
    PathBuf::from(path)
}

#[tauri::command]
pub fn fs_list_dir(path: String) -> Result<Vec<FsEntry>, String> {
    let dir = expand_tilde(&path);
    let read = std::fs::read_dir(&dir).map_err(|e| format!("{}: {}", dir.display(), e))?;
    let mut entries: Vec<FsEntry> = read
        .filter_map(|e| e.ok())
        .map(|e| {
            let is_dir = e.file_type().map(|t| t.is_dir()).unwrap_or(false);
            FsEntry {
                name: e.file_name().to_string_lossy().to_string(),
                path: e.path().to_string_lossy().to_string(),
                is_dir,
            }
        })
        .collect();
    entries.sort_by(|a, b| {
        b.is_dir
            .cmp(&a.is_dir)
            .then(a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    entries.truncate(MAX_ENTRIES);
    Ok(entries)
}

fn mime_for(path: &std::path::Path) -> String {
    let ext = path
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    match ext.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        "bmp" => "image/bmp",
        _ => "application/octet-stream",
    }
    .to_string()
}

#[tauri::command]
pub fn fs_read_file_base64(path: String) -> Result<FileBlob, String> {
    let file = expand_tilde(&path);
    let meta = std::fs::metadata(&file).map_err(|e| format!("{}: {}", file.display(), e))?;
    if meta.len() > MAX_FILE_BYTES {
        return Err(format!(
            "File too large ({:.1} MB, 10 MB max)",
            meta.len() as f64 / 1_048_576.0
        ));
    }
    let bytes = std::fs::read(&file).map_err(|e| format!("{}: {}", file.display(), e))?;
    Ok(FileBlob {
        base64: base64::engine::general_purpose::STANDARD.encode(&bytes),
        mime: mime_for(&file),
        size: meta.len(),
    })
}

#[tauri::command]
pub async fn pick_file(app: tauri::AppHandle) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;
    let file = app.dialog().file().blocking_pick_file();
    Ok(file
        .and_then(|f| f.into_path().ok())
        .map(|p| p.to_string_lossy().to_string()))
}

#[tauri::command]
pub fn open_url(url: String) -> Result<(), String> {
    if !url.starts_with("http://") && !url.starts_with("https://") {
        return Err("Invalid URL — http(s) only".to_string());
    }
    std::process::Command::new("xdg-open")
        .arg(&url)
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("xdg-open failed: {}", e))
}
