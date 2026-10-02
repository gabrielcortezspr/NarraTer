//! Workspaces = scenes with a working directory, filed in the sidebar under
//! groups and folders (Maestri's maestri-workspace). The layout lives in
//! `~/.config/narrater/workspaces.json`, owned by the backend — the scene
//! files themselves stay the canvas' business (auto-save).

use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::{AppHandle, Emitter};

use crate::pty::PtyStateInner;

#[derive(Serialize, Deserialize, Default, Clone, Debug, PartialEq)]
pub struct SceneMeta {
    /// Working directory (absolute)
    #[serde(default)]
    pub dir: Option<String>,
    #[serde(default)]
    pub group: Option<String>,
    #[serde(default)]
    pub folder: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct FolderMeta {
    pub name: String,
    /// None = at the sidebar root
    #[serde(default)]
    pub group: Option<String>,
}

#[derive(Serialize, Deserialize, Default, Clone, Debug)]
pub struct Layout {
    #[serde(default)]
    pub groups: Vec<String>,
    #[serde(default)]
    pub folders: Vec<FolderMeta>,
    #[serde(default)]
    pub scenes: BTreeMap<String, SceneMeta>,
}

static LOCK: Mutex<()> = Mutex::new(());

fn layout_path() -> PathBuf {
    dirs::config_dir().unwrap_or_else(|| PathBuf::from(".")).join("narrater/workspaces.json")
}

fn load() -> Layout {
    std::fs::read_to_string(layout_path())
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default()
}

fn save(layout: &Layout) -> Result<(), String> {
    let path = layout_path();
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let json = serde_json::to_string_pretty(layout).map_err(|e| e.to_string())?;
    std::fs::write(path, json).map_err(|e| e.to_string())
}

/// Load-modify-save under the lock.
fn update<T>(f: impl FnOnce(&mut Layout) -> Result<T, String>) -> Result<T, String> {
    let _guard = LOCK.lock().unwrap();
    let mut layout = load();
    let out = f(&mut layout)?;
    save(&layout)?;
    Ok(out)
}

pub fn rename_scene_meta(old: &str, new: &str) {
    let _ = update(|l| {
        if let Some(meta) = l.scenes.remove(old) {
            l.scenes.insert(new.to_string(), meta);
        }
        Ok(())
    });
}

pub fn forget_scene(name: &str) {
    let _ = update(|l| {
        l.scenes.remove(name);
        Ok(())
    });
}

fn expand(dir: &str) -> PathBuf {
    match (dir.trim().strip_prefix("~"), dirs::home_dir()) {
        (Some(rest), Some(home)) => home.join(rest.trim_start_matches('/')),
        _ => PathBuf::from(dir.trim()),
    }
}

/// Existing directory → absolute path string.
fn existing_dir(dir: &str) -> Result<String, String> {
    let p = expand(dir);
    if !p.is_dir() {
        return Err(format!("'{}' is not an existing directory — create it first (and `git init` if wanted)", dir));
    }
    Ok(p.canonicalize().unwrap_or(p).display().to_string())
}

fn scene_names() -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(crate::scene::scenes_dir())
        .map(|rd| {
            rd.flatten()
                .filter_map(|e| e.file_name().to_str().and_then(|n| n.strip_suffix(".json")).map(String::from))
                .collect()
        })
        .unwrap_or_default();
    if !names.iter().any(|n| n == "default") {
        names.insert(0, "default".into());
    }
    names.sort_by_key(|n| (n != "default", n.to_lowercase()));
    names
}

fn floor_count(scene: &str) -> usize {
    std::fs::read_to_string(crate::scene::scenes_dir().join(format!("{}.json", scene)))
        .ok()
        .and_then(|raw| serde_json::from_str::<Value>(&raw).ok())
        .and_then(|v| v["floors"].as_array().map(Vec::len))
        .unwrap_or(0)
}

#[derive(Debug, PartialEq)]
enum Place {
    Root,
    Group(String),
    Folder { name: String, group: Option<String> },
}

/// Maestri's filing rules: `--group` alone files under a group; `--group` +
/// `--folder` a folder inside that group; `--folder` alone matches a folder
/// anywhere (created at the root when missing); `--root` + `--folder` only
/// root folders; `--root` alone moves out of everything.
fn resolve_place(layout: &Layout, group: Option<&str>, folder: Option<&str>, root: bool) -> Result<Option<Place>, String> {
    if root && group.is_some() {
        return Err("--root and --group don't go together".into());
    }
    Ok(match (group, folder, root) {
        (None, None, false) => None,
        (None, None, true) => Some(Place::Root),
        (Some(g), None, _) => Some(Place::Group(g.to_string())),
        (Some(g), Some(f), _) => Some(Place::Folder { name: f.to_string(), group: Some(g.to_string()) }),
        (None, Some(f), true) => Some(Place::Folder { name: f.to_string(), group: None }),
        (None, Some(f), false) => {
            let hits: Vec<&FolderMeta> = layout.folders.iter().filter(|x| x.name == f).collect();
            match hits.as_slice() {
                [] => Some(Place::Folder { name: f.to_string(), group: None }),
                [one] => Some(Place::Folder { name: f.to_string(), group: one.group.clone() }),
                _ => return Err(format!("folder '{}' exists in several places — add --group \"…\" or --root", f)),
            }
        }
    })
}

/// Files `scene` at `place`, creating the group/folder when missing.
fn file_scene(layout: &mut Layout, scene: &str, place: Place) {
    let meta = layout.scenes.entry(scene.to_string()).or_default();
    match place {
        Place::Root => {
            meta.group = None;
            meta.folder = None;
        }
        Place::Group(g) => {
            meta.group = Some(g.clone());
            meta.folder = None;
            if !layout.groups.contains(&g) {
                layout.groups.push(g);
            }
        }
        Place::Folder { name, group } => {
            meta.group = group.clone();
            meta.folder = Some(name.clone());
            if let Some(g) = &group {
                if !layout.groups.contains(g) {
                    layout.groups.push(g.clone());
                }
            }
            let folder = FolderMeta { name, group };
            if !layout.folders.contains(&folder) {
                layout.folders.push(folder);
            }
        }
    }
}

/// The sidebar hierarchy as text (`workspace list`).
fn render(layout: &Layout, current: Option<&str>) -> String {
    let names = scene_names();
    let line = |name: &str, indent: &str| {
        let meta = layout.scenes.get(name).cloned().unwrap_or_default();
        let floors = floor_count(name);
        format!(
            "{}- {}  {}{}{}",
            indent,
            name,
            meta.dir.as_deref().unwrap_or("(no directory)"),
            if floors > 0 { format!("  ({} floor{})", floors, if floors == 1 { "" } else { "s" }) } else { String::new() },
            if Some(name) == current { "  ← you are here" } else { "" },
        )
    };
    let in_place = |group: Option<&str>, folder: Option<&str>| -> Vec<&String> {
        names
            .iter()
            .filter(|n| {
                let m = layout.scenes.get(*n).cloned().unwrap_or_default();
                m.group.as_deref() == group && m.folder.as_deref() == folder
            })
            .collect()
    };
    let mut out = Vec::new();
    for n in in_place(None, None) {
        out.push(line(n, ""));
    }
    for f in layout.folders.iter().filter(|f| f.group.is_none()) {
        out.push(format!("Folder {}/", f.name));
        let items = in_place(None, Some(&f.name));
        if items.is_empty() {
            out.push("  (empty)".into());
        }
        for n in items {
            out.push(line(n, "  "));
        }
    }
    for g in &layout.groups {
        out.push(format!("Group {}", g));
        for f in layout.folders.iter().filter(|f| f.group.as_deref() == Some(g.as_str())) {
            out.push(format!("  Folder {}/", f.name));
            let items = in_place(Some(g), Some(&f.name));
            if items.is_empty() {
                out.push("    (empty)".into());
            }
            for n in items {
                out.push(line(n, "    "));
            }
        }
        let direct = in_place(Some(g), None);
        let has_folders = layout.folders.iter().any(|f| f.group.as_deref() == Some(g.as_str()));
        if direct.is_empty() && !has_folders {
            out.push("  (empty)".into());
        }
        for n in direct {
            out.push(line(n, "  "));
        }
    }
    out.join("\n")
}

/// Deep copy of a scene for `workspace create --from`: fresh ids, paths under
/// the source directory retargeted onto the new one. Floors come over as
/// plain floors (a git worktree belongs to the source repository).
fn clone_scene(src: &Value, from_dir: Option<&str>, to_dir: &str) -> Value {
    let mut out = src.clone();
    let retarget = |v: &mut Value, floor_dirs: &HashMap<String, String>| {
        if let Some(s) = v.as_str() {
            if let Some(nd) = floor_dirs.get(s) {
                *v = Value::String(nd.clone());
            } else if let Some(rest) = from_dir.and_then(|fd| s.strip_prefix(fd)).filter(|r| r.is_empty() || r.starts_with('/')) {
                *v = Value::String(format!("{}{}", to_dir, rest));
            }
        }
    };
    // Floor directories (worktrees) map onto the new root
    let mut floor_dirs = HashMap::new();
    if let Some(floors) = out["floors"].as_array_mut() {
        for f in floors.iter_mut() {
            if let Some(d) = f["dir"].as_str() {
                floor_dirs.insert(d.to_string(), to_dir.to_string());
            }
            f["dir"] = Value::Null;
            f["worktree"] = Value::Null;
            f["branch"] = Value::Null;
        }
    }
    let mut ids = HashMap::new();
    if let Some(nodes) = out["nodes"].as_array_mut() {
        for n in nodes.iter_mut() {
            let kind = n["node_type"].as_str().unwrap_or("node").to_string();
            let new_id = format!("{}-{}", kind, &uuid::Uuid::new_v4().simple().to_string()[..8]);
            if let Some(old) = n["id"].as_str() {
                ids.insert(old.to_string(), new_id.clone());
            }
            n["id"] = Value::String(new_id);
            for key in ["cwd", "path"] {
                retarget(&mut n[key], &floor_dirs);
            }
        }
    }
    if let Some(edges) = out["edges"].as_array_mut() {
        edges.retain(|e| {
            ids.contains_key(e["source"].as_str().unwrap_or("")) && ids.contains_key(e["target"].as_str().unwrap_or(""))
        });
        for e in edges.iter_mut() {
            for key in ["source", "target"] {
                if let Some(new) = e[key].as_str().and_then(|old| ids.get(old)).cloned() {
                    e[key] = Value::String(new);
                }
            }
            e["id"] = Value::String(format!("edge-{}", &uuid::Uuid::new_v4().simple().to_string()[..8]));
        }
    }
    out
}

fn scene_path(name: &str) -> PathBuf {
    crate::scene::scenes_dir().join(format!("{}.json", name))
}

fn valid_name(name: &str) -> Result<&str, String> {
    let n = name.trim();
    if n.is_empty() || n.contains('/') || n.starts_with('.') {
        return Err("a workspace name can't be empty, start with '.' or contain '/'".into());
    }
    Ok(n)
}

fn s<'a>(p: &'a Value, k: &str) -> Option<&'a str> {
    p.get(k).and_then(Value::as_str).map(str::trim).filter(|v| !v.is_empty())
}

const NOT_MAESTRO: &str = "Error: this terminal is not the Maestro — the user must turn on Maestro mode on your terminal (the crown in its header) to unlock this command";

/// `narrater workspace …` (Maestro only).
pub fn run_cli(app: &AppHandle, pty: &Arc<Mutex<PtyStateInner>>, from: &str, current: Option<String>, p: &Value) -> String {
    if !pty.lock().unwrap().maestros.contains(from) {
        return NOT_MAESTRO.into();
    }
    let root = p.get("root").and_then(Value::as_bool).unwrap_or(false);
    let result: Result<String, String> = (|| match s(p, "verb").unwrap_or("") {
        "list" => Ok(render(&load(), current.as_deref())),
        "create" => {
            let name = valid_name(s(p, "name").unwrap_or(""))?.to_string();
            let dir = existing_dir(s(p, "dir").ok_or("a workspace needs --dir PATH")?)?;
            if scene_names().contains(&name) {
                return Err(format!("a workspace named '{}' already exists", name));
            }
            let data = match s(p, "from") {
                Some(src) => {
                    if !scene_names().iter().any(|n| n == src) {
                        return Err(format!("no workspace named '{}' — run `narrater workspace list`", src));
                    }
                    let raw = std::fs::read_to_string(scene_path(src)).unwrap_or_else(|_| "{\"nodes\":[],\"edges\":[]}".into());
                    let v: Value = serde_json::from_str(&raw).map_err(|e| format!("cannot read '{}': {}", src, e))?;
                    let src_dir = load().scenes.get(src).and_then(|m| m.dir.clone());
                    clone_scene(&v, src_dir.as_deref(), &dir)
                }
                None => serde_json::json!({ "nodes": [], "edges": [] }),
            };
            std::fs::create_dir_all(crate::scene::scenes_dir()).map_err(|e| e.to_string())?;
            std::fs::write(scene_path(&name), serde_json::to_string_pretty(&data).unwrap_or_default())
                .map_err(|e| e.to_string())?;
            let place = update(|l| {
                let place = resolve_place(l, s(p, "group"), s(p, "folder"), root)?;
                l.scenes.entry(name.clone()).or_default().dir = Some(dir.clone());
                if let Some(pl) = place {
                    file_scene(l, &name, pl);
                }
                Ok(())
            });
            if let Err(e) = place {
                let _ = std::fs::remove_file(scene_path(&name));
                return Err(e);
            }
            Ok(format!(
                "ok: workspace '{}' created at {}{} — it's in the sidebar; terminals start when the user opens it",
                name,
                dir,
                if s(p, "from").is_some() { format!(" (cloned from '{}')", s(p, "from").unwrap()) } else { String::new() }
            ))
        }
        "move" => {
            let name = s(p, "name").ok_or("which workspace?")?.to_string();
            if !scene_names().contains(&name) {
                return Err(format!("no workspace named '{}' — run `narrater workspace list`", name));
            }
            update(|l| {
                let place = resolve_place(l, s(p, "group"), s(p, "folder"), root)?
                    .ok_or("pass --group \"G\", --folder \"F\" or --root")?;
                file_scene(l, &name, place);
                Ok(format!("ok: '{}' moved", name))
            })
        }
        "rename" => {
            let old = s(p, "name").ok_or("which workspace?")?.to_string();
            let new = valid_name(s(p, "new_name").unwrap_or(""))?.to_string();
            let names = scene_names();
            if !names.contains(&old) {
                return Err(format!("no workspace named '{}' — valid names:\n{}", old, names.join("\n")));
            }
            if old == "default" {
                return Err("the default workspace can't be renamed".into());
            }
            if names.iter().any(|n| n != &old && n.eq_ignore_ascii_case(&new)) {
                return Err(format!("a workspace named '{}' already exists", new));
            }
            // The frontend renames it (scene file, routines, layout) so its
            // own state follows when it's the open one
            let _ = app.emit("workspace_rename_request", serde_json::json!({ "old": old, "new": new }));
            Ok(format!("ok: '{}' is now '{}'", old, new))
        }
        _ => Err("unknown workspace command — run `narrater help workspace`".into()),
    })();
    match result {
        Ok(text) => {
            let _ = app.emit("workspaces_changed", ());
            text
        }
        Err(e) => format!("Error: {}", e),
    }
}

// ---- Tauri commands (sidebar) --------------------------------------------

#[tauri::command]
pub fn workspaces_layout() -> Layout {
    load()
}

/// Sets (or clears) a workspace's working directory.
#[tauri::command]
pub fn workspace_set_dir(name: String, dir: Option<String>) -> Result<Option<String>, String> {
    let dir = match dir.as_deref().map(str::trim).filter(|d| !d.is_empty()) {
        Some(d) => Some(existing_dir(d)?),
        None => None,
    };
    update(|l| {
        l.scenes.entry(name).or_default().dir = dir.clone();
        Ok(dir)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn layout() -> Layout {
        Layout {
            groups: vec!["Data".into()],
            folders: vec![
                FolderMeta { name: "Reports".into(), group: Some("Data".into()) },
                FolderMeta { name: "Archive".into(), group: None },
                FolderMeta { name: "Dup".into(), group: None },
                FolderMeta { name: "Dup".into(), group: Some("Data".into()) },
            ],
            scenes: BTreeMap::new(),
        }
    }

    #[test]
    fn filing_rules() {
        let l = layout();
        assert_eq!(resolve_place(&l, None, None, false), Ok(None));
        assert_eq!(resolve_place(&l, None, None, true), Ok(Some(Place::Root)));
        assert_eq!(resolve_place(&l, Some("Data"), None, false), Ok(Some(Place::Group("Data".into()))));
        // --folder alone finds it in its group
        assert_eq!(
            resolve_place(&l, None, Some("Reports"), false),
            Ok(Some(Place::Folder { name: "Reports".into(), group: Some("Data".into()) }))
        );
        // Missing → created at the root
        assert_eq!(
            resolve_place(&l, None, Some("New"), false),
            Ok(Some(Place::Folder { name: "New".into(), group: None }))
        );
        assert!(resolve_place(&l, None, Some("Dup"), false).unwrap_err().contains("--group"));
        assert_eq!(
            resolve_place(&l, None, Some("Dup"), true),
            Ok(Some(Place::Folder { name: "Dup".into(), group: None }))
        );
        assert!(resolve_place(&l, Some("Data"), None, true).is_err());
    }

    #[test]
    fn filing_creates_groups_and_folders() {
        let mut l = Layout::default();
        file_scene(&mut l, "q3", Place::Folder { name: "Reports".into(), group: Some("Data".into()) });
        assert_eq!(l.groups, vec!["Data".to_string()]);
        assert_eq!(l.folders, vec![FolderMeta { name: "Reports".into(), group: Some("Data".into()) }]);
        assert_eq!(l.scenes["q3"].folder.as_deref(), Some("Reports"));
        file_scene(&mut l, "q3", Place::Root);
        assert_eq!((l.scenes["q3"].group.clone(), l.scenes["q3"].folder.clone()), (None, None));
    }

    #[test]
    fn clone_remaps_ids_and_paths() {
        let src = json!({
            "nodes": [
                { "id": "terminal-aaaa1111", "node_type": "terminal", "cwd": "/w/src/app" },
                { "id": "note-bbbb2222", "node_type": "note" },
                { "id": "terminal-cccc3333", "node_type": "terminal", "cwd": "/elsewhere", "floor": "floor-1" },
                { "id": "terminal-dddd4444", "node_type": "terminal", "cwd": "/wt/exp", "floor": "floor-1" },
                { "id": "filetree-eeee5555", "node_type": "filetree", "path": "/w/srcfoo" }
            ],
            "edges": [
                { "id": "edge-1", "source": "terminal-aaaa1111", "target": "note-bbbb2222" },
                { "id": "edge-2", "source": "terminal-aaaa1111", "target": "gone" }
            ],
            "floors": [{ "id": "floor-1", "name": "Exp", "branch": "exp", "worktree": "/wt/exp", "dir": "/wt/exp" }]
        });
        let out = clone_scene(&src, Some("/w/src"), "/new");
        let nodes = out["nodes"].as_array().unwrap();
        assert_ne!(nodes[0]["id"], "terminal-aaaa1111");
        assert!(nodes[0]["id"].as_str().unwrap().starts_with("terminal-"));
        assert_eq!(nodes[0]["cwd"], "/new/app");
        assert_eq!(nodes[2]["cwd"], "/elsewhere");
        assert_eq!(nodes[3]["cwd"], "/new", "floor worktree dirs map onto the new root");
        assert_eq!(nodes[4]["path"], "/w/srcfoo", "a sibling path with the same prefix is not retargeted");
        let edges = out["edges"].as_array().unwrap();
        assert_eq!(edges.len(), 1, "edges to missing nodes are dropped");
        assert_eq!(edges[0]["source"], nodes[0]["id"]);
        assert_eq!(edges[0]["target"], nodes[1]["id"]);
        assert_eq!(out["floors"][0]["branch"], Value::Null);
    }
}
