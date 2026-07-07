use dirs::config_dir;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SceneNode {
    pub id: String,
    pub node_type: String,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub agent_type: Option<String>,
    pub command: Option<String>,
    pub label: Option<String>,
    pub content: Option<String>,
    pub instructions: Option<String>,
    pub schedule_command: Option<String>,
    pub schedule_interval_secs: Option<u64>,
    pub role_id: Option<String>,
    pub role_name: Option<String>,
    pub role_color: Option<String>,
    #[serde(default)]
    pub skip_permissions: Option<bool>,
    // filetree.rootPath / attachment.path
    #[serde(default)]
    pub path: Option<String>,
    // portal
    #[serde(default)]
    pub url: Option<String>,
    // filetree
    #[serde(default)]
    pub expanded_paths: Option<Vec<String>>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SceneEdge {
    pub id: String,
    pub source: String,
    pub target: String,
    pub edge_type: Option<String>,
    #[serde(default)]
    pub source_handle: Option<String>,
    #[serde(default)]
    pub target_handle: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct SceneData {
    pub nodes: Vec<SceneNode>,
    pub edges: Vec<SceneEdge>,
}

fn scenes_dir() -> PathBuf {
    let base = config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("narrater");
    let dir = base.join("scenes");
    // Scenes used to be called "histórias" — migrate the old dir once.
    if !dir.exists() {
        let legacy = base.join("historias");
        if legacy.exists() {
            let _ = fs::rename(&legacy, &dir);
        }
    }
    dir
}

#[tauri::command]
pub fn load_scene(name: String) -> Result<SceneData, String> {
    let path = scenes_dir().join(format!("{}.json", name));
    if !path.exists() {
        return Ok(SceneData::default());
    }
    let raw = fs::read_to_string(&path).map_err(|e| e.to_string())?;
    serde_json::from_str(&raw).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn save_scene(name: String, data: SceneData) -> Result<(), String> {
    let dir = scenes_dir();
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join(format!("{}.json", name));
    let json = serde_json::to_string_pretty(&data).map_err(|e| e.to_string())?;
    fs::write(&path, json).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_scenes() -> Result<Vec<String>, String> {
    let dir = scenes_dir();
    if !dir.exists() {
        return Ok(vec![]);
    }
    let names = fs::read_dir(&dir)
        .map_err(|e| e.to_string())?
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            name.ends_with(".json").then(|| name.trim_end_matches(".json").to_string())
        })
        .collect();
    Ok(names)
}

#[tauri::command]
pub fn delete_scene(name: String) -> Result<(), String> {
    let path = scenes_dir().join(format!("{}.json", name));
    if path.exists() {
        fs::remove_file(&path).map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub fn rename_scene(old_name: String, new_name: String) -> Result<(), String> {
    let dir = scenes_dir();
    let old_path = dir.join(format!("{}.json", old_name));
    let new_path = dir.join(format!("{}.json", new_name));
    if old_path.exists() {
        fs::rename(&old_path, &new_path).map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub fn open_in_editor(editor: String, path: String) -> Result<(), String> {
    std::process::Command::new(&editor)
        .arg(&path)
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("Failed to open '{}': {}", editor, e))
}
