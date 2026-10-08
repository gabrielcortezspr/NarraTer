use dirs::config_dir;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use tauri::State;

use crate::routines::{self, RoutinesState};

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
    /// Terminal working directory (the project the agent works on)
    #[serde(default)]
    pub cwd: Option<String>,
    /// Maestro mode: the terminal may recruit/dismiss teammates and manage roles
    #[serde(default)]
    pub maestro: Option<bool>,
    /// Floor the node lives on (None = the ground floor)
    #[serde(default)]
    pub floor: Option<String>,
    // filetree.rootPath / attachment.path
    #[serde(default)]
    pub path: Option<String>,
    // portal
    #[serde(default)]
    pub url: Option<String>,
    /// Device portal: AVD name or adb serial
    #[serde(default)]
    pub device: Option<String>,
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

/// A floor: another canvas level of the scene, optionally backed by a git
/// worktree on its own branch (isolation within a project).
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct FloorMeta {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub branch: Option<String>,
    /// The worktree root (what `git worktree remove` takes)
    #[serde(default)]
    pub worktree: Option<String>,
    /// Where terminals of this floor work (the worktree, at the scene's
    /// subdirectory when the scene root is inside the repo)
    #[serde(default)]
    pub dir: Option<String>,
    /// The checkout is a full copy of the repository (`--copy`), not a worktree
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub copy: Option<bool>,
    /// The pull request the floor reviews (`--pull-request`)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pull_request: Option<crate::floors::PullRequestInfo>,
    /// The line agents post with `narrater floor status`
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<FloorStatusLine>,
}

/// Where a floor's work stands: one short line, a state and an optional
/// progress ("2/5", "40%"), shown under the floor in the floor bar.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct FloorStatusLine {
    #[serde(default)]
    pub text: String,
    /// working | blocked | review | done
    pub state: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub progress: Option<String>,
    /// Epoch ms of the last post
    #[serde(default)]
    pub at: u64,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct SceneData {
    pub nodes: Vec<SceneNode>,
    pub edges: Vec<SceneEdge>,
    #[serde(default)]
    pub floors: Vec<FloorMeta>,
    /// Status line of the ground level (a summary of the floors above)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ground_status: Option<FloorStatusLine>,
}

pub(crate) fn scenes_dir() -> PathBuf {
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
pub fn delete_scene(
    name: String,
    routines_state: State<'_, RoutinesState>,
    pty: State<'_, crate::pty::PtyState>,
) -> Result<(), String> {
    routines::delete_scene(&name, &routines_state);
    pty.0.lock().unwrap().links.delete_scene(&name);
    crate::workspaces::forget_scene(&name);
    let path = scenes_dir().join(format!("{}.json", name));
    if path.exists() {
        fs::remove_file(&path).map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub fn rename_scene(
    old_name: String,
    new_name: String,
    routines_state: State<'_, RoutinesState>,
    pty: State<'_, crate::pty::PtyState>,
) -> Result<(), String> {
    routines::rename_scene(&old_name, &new_name, &routines_state);
    {
        let mut inner = pty.0.lock().unwrap();
        inner.names.rename_scene(&old_name, &new_name);
        inner.links.rename_scene(&old_name, &new_name);
    }
    crate::workspaces::rename_scene_meta(&old_name, &new_name);
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
