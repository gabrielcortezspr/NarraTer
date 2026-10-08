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
    /// Freehand drawings, in canvas coordinates
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub drawings: Vec<Drawing>,
}

/// A pen stroke drawn on the canvas: points are [x, y, pressure].
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Drawing {
    pub points: Vec<Vec<f64>>,
    pub color: String,
    pub size: f64,
    /// Floor it was drawn on (None = the ground floor)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub floor: Option<String>,
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
    backup_before_write(&name, &path, false);
    write_atomic(&path, &json)
}

/// Temp file + rename: a crash mid-save never leaves a truncated scene.
fn write_atomic(path: &std::path::Path, content: &str) -> Result<(), String> {
    let tmp = path.with_extension(format!("json.tmp-{}", std::process::id()));
    fs::write(&tmp, content).map_err(|e| e.to_string())?;
    fs::rename(&tmp, path).map_err(|e| {
        let _ = fs::remove_file(&tmp);
        e.to_string()
    })
}

// ---- Backups ----------------------------------------------------------------

/// At most one backup per scene this often (saves happen every second).
const BACKUP_EVERY_SECS: u64 = 10 * 60;
const BACKUPS_KEPT: usize = 30;

pub(crate) fn backups_dir(name: &str) -> PathBuf {
    config_dir().unwrap_or_else(|| PathBuf::from(".")).join("narrater/backups").join(name)
}

fn backup_files(name: &str) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = fs::read_dir(backups_dir(name))
        .map(|rd| rd.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "json")).collect())
        .unwrap_or_default();
    files.sort();
    files
}

/// Copies the scene file about to be overwritten into its backups (unless
/// one was made recently, or `force`). Only a file that parses is kept — a
/// corrupt one would be no use to restore.
fn backup_before_write(name: &str, path: &std::path::Path, force: bool) {
    let Ok(current) = fs::read_to_string(path) else { return };
    if serde_json::from_str::<SceneData>(&current).is_err() {
        return;
    }
    let files = backup_files(name);
    let recent = files.last().and_then(|f| f.metadata().ok()?.modified().ok()?.elapsed().ok());
    if !force && recent.is_some_and(|age| age.as_secs() < BACKUP_EVERY_SECS) {
        return;
    }
    // Identical to the last backup: nothing new to keep
    if files.last().and_then(|f| fs::read_to_string(f).ok()).as_deref() == Some(current.as_str()) {
        return;
    }
    let dir = backups_dir(name);
    if fs::create_dir_all(&dir).is_err() {
        return;
    }
    let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S%.3f");
    let _ = fs::write(dir.join(format!("{}.json", stamp)), &current);
    let files = backup_files(name);
    if files.len() > BACKUPS_KEPT {
        for old in &files[..files.len() - BACKUPS_KEPT] {
            let _ = fs::remove_file(old);
        }
    }
}

#[derive(Serialize)]
pub struct BackupInfo {
    pub file: String,
    /// Epoch ms
    pub at: u64,
    pub nodes: usize,
    pub terminals: usize,
    pub floors: usize,
}

/// A scene's backups, newest first.
#[tauri::command]
pub fn scene_backups(name: String) -> Vec<BackupInfo> {
    let mut out: Vec<BackupInfo> = backup_files(&name)
        .into_iter()
        .filter_map(|p| {
            let data: SceneData = serde_json::from_str(&fs::read_to_string(&p).ok()?).ok()?;
            let at = p.metadata().ok()?.modified().ok()?.duration_since(std::time::UNIX_EPOCH).ok()?.as_millis() as u64;
            Some(BackupInfo {
                file: p.file_name()?.to_string_lossy().into_owned(),
                at,
                nodes: data.nodes.len(),
                terminals: data.nodes.iter().filter(|n| n.node_type == "terminal").count(),
                floors: data.floors.len(),
            })
        })
        .collect();
    out.reverse();
    out
}

/// Puts a backup back as the scene file; what was there becomes a backup.
#[tauri::command]
pub fn scene_restore_backup(name: String, file: String) -> Result<(), String> {
    if file.contains('/') || file.contains("..") {
        return Err("invalid backup".into());
    }
    let src = backups_dir(&name).join(&file);
    let content = fs::read_to_string(&src).map_err(|e| format!("can't read the backup: {}", e))?;
    serde_json::from_str::<SceneData>(&content).map_err(|e| format!("the backup is unreadable: {}", e))?;
    let path = scenes_dir().join(format!("{}.json", name));
    backup_before_write(&name, &path, true);
    write_atomic(&path, &content)
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
    let (old_b, new_b) = (backups_dir(&old_name), backups_dir(&new_name));
    if old_b.exists() && !new_b.exists() {
        let _ = fs::rename(old_b, new_b);
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

#[cfg(test)]
mod tests {
    use super::*;

    fn scene(nodes: usize) -> SceneData {
        let mut d = SceneData::default();
        for i in 0..nodes {
            d.nodes.push(serde_json::from_value(serde_json::json!({
                "id": format!("n{}", i), "node_type": "note", "x": 0.0, "y": 0.0, "width": 1.0, "height": 1.0,
                "agent_type": null, "command": null, "label": null, "content": null, "instructions": null,
                "schedule_command": null, "schedule_interval_secs": null, "role_id": null, "role_name": null, "role_color": null
            })).unwrap());
        }
        d
    }

    #[test]
    fn backups_keep_valid_versions_and_restore() {
        let name = format!("backup-test-{}", uuid::Uuid::new_v4().simple());
        let path = scenes_dir().join(format!("{}.json", name));
        save_scene(name.clone(), scene(1)).unwrap();
        // First overwrite backs up the previous version; the next one, minutes
        // later in real life, is skipped here (too recent)
        save_scene(name.clone(), scene(2)).unwrap();
        save_scene(name.clone(), scene(3)).unwrap();
        let list = scene_backups(name.clone());
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].nodes, 1);
        // A corrupt scene file is never backed up
        fs::write(&path, "{ not json").unwrap();
        backup_before_write(&name, &path, true);
        assert_eq!(scene_backups(name.clone()).len(), 1);
        // Restore: the backup comes back
        scene_restore_backup(name.clone(), list[0].file.clone()).unwrap();
        assert_eq!(load_scene(name.clone()).unwrap().nodes.len(), 1);
        assert!(scene_restore_backup(name.clone(), "../x.json".into()).is_err());
        let _ = fs::remove_file(&path);
        let _ = fs::remove_dir_all(backups_dir(&name));
    }
}
