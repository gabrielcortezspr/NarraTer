use dirs::config_dir;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct HistoriaNode {
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
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct HistoriaEdge {
    pub id: String,
    pub source: String,
    pub target: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct HistoriaData {
    pub nodes: Vec<HistoriaNode>,
    pub edges: Vec<HistoriaEdge>,
}

fn historias_dir() -> PathBuf {
    config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("narrater")
        .join("historias")
}

#[tauri::command]
pub fn load_historia(name: String) -> Result<HistoriaData, String> {
    let path = historias_dir().join(format!("{}.json", name));
    if !path.exists() {
        return Ok(HistoriaData::default());
    }
    let raw = fs::read_to_string(&path).map_err(|e| e.to_string())?;
    serde_json::from_str(&raw).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn save_historia(name: String, data: HistoriaData) -> Result<(), String> {
    let dir = historias_dir();
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join(format!("{}.json", name));
    let json = serde_json::to_string_pretty(&data).map_err(|e| e.to_string())?;
    fs::write(&path, json).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_historias() -> Result<Vec<String>, String> {
    let dir = historias_dir();
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
