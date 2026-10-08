//! Who is who across live scenes. Several workspaces run at once (like
//! Maestri's), so terminal names are unique *within a scene*: "Dev" in one
//! workspace and "Dev" in another are different agents, addressed from
//! outside as `Dev @ Workspace`.
//!
//! Also the cross-workspace links: a teammate recruited into another
//! workspace (`narrater recruit --workspace`) stays wired to its recruiter.
//! Canvas edges can't span scenes, so those links live here, persisted in
//! ~/.config/narrater/links.json.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Separates a name from its workspace in an address: `Dev @ Frontend`.
pub const AT: &str = " @ ";

/// Terminal names by scene.
#[derive(Default)]
pub struct Names {
    labels: HashMap<String, String>,
    scenes: HashMap<String, String>,
    index: HashMap<(String, String), String>,
}

impl Names {
    /// Registers `id` in `scene` as `label`, suffixing it (`-2`, `-3`…) when
    /// another terminal of that scene has it. Returns the name it got.
    pub fn set(&mut self, id: &str, scene: &str, label: &str) -> String {
        self.remove(id);
        let mut candidate = label.to_string();
        let mut n = 2;
        while self.index.get(&(scene.to_string(), candidate.clone())).is_some_and(|owner| owner != id) {
            candidate = format!("{}-{}", label, n);
            n += 1;
        }
        self.labels.insert(id.to_string(), candidate.clone());
        self.scenes.insert(id.to_string(), scene.to_string());
        self.index.insert((scene.to_string(), candidate.clone()), id.to_string());
        candidate
    }

    /// Renames `id` in its scene; refuses a name another terminal there has.
    pub fn rename(&mut self, id: &str, label: &str) -> Result<(), String> {
        let scene = self.scene_of(id);
        if self.index.get(&(scene.clone(), label.to_string())).is_some_and(|owner| owner != id) {
            return Err(format!("Label '{}' is already used by another terminal", label));
        }
        self.set(id, &scene, label);
        Ok(())
    }

    pub fn remove(&mut self, id: &str) {
        if let (Some(label), Some(scene)) = (self.labels.remove(id), self.scenes.remove(id)) {
            if self.index.get(&(scene.clone(), label.clone())).is_some_and(|owner| owner == id) {
                self.index.remove(&(scene, label));
            }
        }
    }

    /// A scene was renamed: its terminals follow.
    pub fn rename_scene(&mut self, old: &str, new: &str) {
        let ids: Vec<String> = self.scenes.iter().filter(|(_, s)| *s == old).map(|(id, _)| id.clone()).collect();
        for id in ids {
            let label = self.label_of(&id);
            self.set(&id, new, &label);
        }
    }

    pub fn label_of(&self, id: &str) -> String {
        self.labels.get(id).cloned().unwrap_or_else(|| id.to_string())
    }

    pub fn scene_of(&self, id: &str) -> String {
        self.scenes.get(id).cloned().unwrap_or_default()
    }

    /// How `viewer` should address `id`: its bare name in the same scene,
    /// `Name @ Scene` from another one.
    pub fn address(&self, id: &str, viewer: &str) -> String {
        let scene = self.scene_of(id);
        if scene.is_empty() || scene == self.scene_of(viewer) {
            self.label_of(id)
        } else {
            format!("{}{}{}", self.label_of(id), AT, scene)
        }
    }

    /// Ids named `label` in `scene`: the exact name, else a unique
    /// case-insensitive match.
    fn find_in(&self, scene: &str, label: &str) -> Option<String> {
        if let Some(id) = self.index.get(&(scene.to_string(), label.to_string())) {
            return Some(id.clone());
        }
        let lower = label.to_lowercase();
        let hits: Vec<&String> = self
            .index
            .iter()
            .filter(|((s, l), _)| s == scene && l.to_lowercase() == lower)
            .map(|(_, id)| id)
            .collect();
        (hits.len() == 1).then(|| hits[0].clone())
    }

    /// Resolves an address as `viewer` writes it: `Name @ Scene`, or a bare
    /// name — in the viewer's scene first, then the one terminal of another
    /// scene with that name that `reachable` accepts (a cross-workspace
    /// teammate can be called by its bare name when it's unambiguous).
    pub fn resolve(&self, viewer: &str, address: &str, reachable: impl Fn(&str) -> bool) -> Option<String> {
        let address = address.trim();
        if let Some((name, scene)) = address.rsplit_once(AT).or_else(|| address.rsplit_once('@')) {
            let (name, scene) = (name.trim(), scene.trim());
            if let Some(id) = self.find_in(scene, name).or_else(|| {
                // Scene names are matched case-insensitively too
                let lower = scene.to_lowercase();
                let scenes: HashSet<&String> = self.scenes.values().filter(|s| s.to_lowercase() == lower).collect();
                (scenes.len() == 1).then(|| self.find_in(scenes.into_iter().next().unwrap(), name)).flatten()
            }) {
                return Some(id);
            }
        }
        if let Some(id) = self.find_in(&self.scene_of(viewer), address) {
            return Some(id);
        }
        let lower = address.to_lowercase();
        let hits: Vec<&String> = self
            .index
            .iter()
            .filter(|((_, l), id)| l.to_lowercase() == lower && reachable(id))
            .map(|(_, id)| id)
            .collect();
        (hits.len() == 1).then(|| hits[0].clone())
    }

    /// (id, label, scene) of every named terminal.
    pub fn all(&self) -> Vec<(String, String, String)> {
        self.labels.iter().map(|(id, l)| (id.clone(), l.clone(), self.scene_of(id))).collect()
    }
}

/// One end of a cross-workspace link.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct LinkEnd {
    pub id: String,
    pub scene: String,
    /// Name at the time of linking (shown while the scene isn't running)
    #[serde(default)]
    pub label: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Link {
    pub a: LinkEnd,
    pub b: LinkEnd,
}

fn links_path() -> PathBuf {
    dirs::config_dir().unwrap_or_else(|| PathBuf::from(".")).join("narrater/links.json")
}

/// Cross-workspace links, persisted.
#[derive(Default)]
pub struct Links {
    list: Vec<Link>,
    path: Option<PathBuf>,
}

impl Links {
    pub fn load() -> Self {
        Self::load_from(links_path())
    }

    pub fn load_from(path: PathBuf) -> Self {
        let list = std::fs::read_to_string(&path).ok().and_then(|raw| serde_json::from_str(&raw).ok()).unwrap_or_default();
        Links { list, path: Some(path) }
    }

    fn save(&self) {
        let Some(path) = &self.path else { return };
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Ok(text) = serde_json::to_string_pretty(&self.list) {
            let tmp = path.with_extension("json.tmp");
            if std::fs::write(&tmp, text).is_ok() {
                let _ = std::fs::rename(&tmp, path);
            }
        }
    }

    pub fn list(&self) -> &[Link] {
        &self.list
    }

    pub fn linked(&self, x: &str, y: &str) -> bool {
        self.list.iter().any(|l| (l.a.id == x && l.b.id == y) || (l.a.id == y && l.b.id == x))
    }

    pub fn add(&mut self, a: LinkEnd, b: LinkEnd) {
        if a.id == b.id || self.linked(&a.id, &b.id) {
            return;
        }
        self.list.push(Link { a, b });
        self.save();
    }

    /// A terminal is gone (dismissed, deleted): its links go with it.
    pub fn remove_node(&mut self, id: &str) -> bool {
        let before = self.list.len();
        self.list.retain(|l| l.a.id != id && l.b.id != id);
        let changed = self.list.len() != before;
        if changed {
            self.save();
        }
        changed
    }

    /// Links touching `scene`'s terminals: the other ends' scenes.
    pub fn scenes_linked_to(&self, scene: &str) -> Vec<String> {
        let mut out: Vec<String> = self
            .list
            .iter()
            .flat_map(|l| {
                if l.a.scene == scene && l.b.scene != scene {
                    Some(l.b.scene.clone())
                } else if l.b.scene == scene && l.a.scene != scene {
                    Some(l.a.scene.clone())
                } else {
                    None
                }
            })
            .collect();
        out.sort();
        out.dedup();
        out
    }

    pub fn rename_scene(&mut self, old: &str, new: &str) {
        let mut changed = false;
        for l in self.list.iter_mut() {
            for end in [&mut l.a, &mut l.b] {
                if end.scene == old {
                    end.scene = new.to_string();
                    changed = true;
                }
            }
        }
        if changed {
            self.save();
        }
    }

    pub fn delete_scene(&mut self, scene: &str) {
        let before = self.list.len();
        self.list.retain(|l| l.a.scene != scene && l.b.scene != scene);
        if self.list.len() != before {
            self.save();
        }
    }

    /// Keeps the stored names current (for display while a scene sleeps).
    pub fn refresh_label(&mut self, id: &str, label: &str) {
        let mut changed = false;
        for l in self.list.iter_mut() {
            for end in [&mut l.a, &mut l.b] {
                if end.id == id && end.label != label {
                    end.label = label.to_string();
                    changed = true;
                }
            }
        }
        if changed {
            self.save();
        }
    }
}

// ---- Tauri commands --------------------------------------------------------

use crate::pty::PtyState;
use tauri::State;

#[tauri::command]
pub fn links_list(state: State<'_, PtyState>) -> Vec<Link> {
    state.0.lock().unwrap().links.list().to_vec()
}

#[tauri::command]
pub fn link_add(a: LinkEnd, b: LinkEnd, state: State<'_, PtyState>) {
    state.0.lock().unwrap().links.add(a, b);
}

/// A terminal was removed from its canvas: its cross-workspace links go.
#[tauri::command]
pub fn links_remove_node(id: String, state: State<'_, PtyState>) -> bool {
    state.0.lock().unwrap().links.remove_node(&id)
}

/// Scenes with terminals linked to `scene`'s (they wake with it).
#[tauri::command]
pub fn links_scenes(scene: String, state: State<'_, PtyState>) -> Vec<String> {
    state.0.lock().unwrap().links.scenes_linked_to(&scene)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_unique_per_scene() {
        let mut n = Names::default();
        assert_eq!(n.set("a", "Front", "Dev"), "Dev");
        assert_eq!(n.set("b", "Back", "Dev"), "Dev");
        assert_eq!(n.set("c", "Front", "Dev"), "Dev-2");
        // Re-registering keeps its own name
        assert_eq!(n.set("a", "Front", "Dev"), "Dev");
        assert!(n.rename("c", "Dev").is_err());
        n.rename("c", "Ops").unwrap();
        assert_eq!(n.label_of("c"), "Ops");
        assert_eq!(n.address("b", "a"), "Dev @ Back");
        assert_eq!(n.address("c", "a"), "Ops");
    }

    #[test]
    fn resolves_addresses_across_scenes() {
        let mut n = Names::default();
        n.set("a", "Front", "Lead");
        n.set("b", "Back", "Dev");
        n.set("c", "Front", "Dev");
        n.set("d", "Other", "Solo");
        let any = |_: &str| true;
        // Own scene first
        assert_eq!(n.resolve("a", "dev", any).as_deref(), Some("c"));
        assert_eq!(n.resolve("a", "Dev @ Back", any).as_deref(), Some("b"));
        assert_eq!(n.resolve("a", "dev @ back", any).as_deref(), Some("b"));
        assert_eq!(n.resolve("a", "Dev@Back", any).as_deref(), Some("b"));
        // Bare name of another scene: only when reachable and unambiguous
        assert_eq!(n.resolve("a", "Solo", |id| id == "d").as_deref(), Some("d"));
        assert_eq!(n.resolve("a", "Solo", |_| false), None);
        assert_eq!(n.resolve("b", "Dev", any).as_deref(), Some("b"));
        n.rename_scene("Back", "Backend");
        assert_eq!(n.resolve("a", "Dev @ Backend", any).as_deref(), Some("b"));
        n.remove("b");
        assert_eq!(n.resolve("a", "Dev @ Backend", any), None);
    }

    #[test]
    fn links_persist_and_follow_scenes() {
        let path = std::env::temp_dir().join(format!("narrater-links-{}.json", uuid::Uuid::new_v4()));
        let end = |id: &str, scene: &str| LinkEnd { id: id.into(), scene: scene.into(), label: id.to_uppercase() };
        let mut links = Links::load_from(path.clone());
        links.add(end("a", "Front"), end("b", "Back"));
        links.add(end("b", "Back"), end("a", "Front")); // duplicate, either way round
        assert_eq!(links.list().len(), 1);
        assert!(links.linked("b", "a"));
        assert_eq!(links.scenes_linked_to("Front"), vec!["Back".to_string()]);
        links.rename_scene("Back", "Backend");
        links.refresh_label("b", "Builder");
        let reloaded = Links::load_from(path.clone());
        assert_eq!(reloaded.list()[0].b.scene, "Backend");
        assert_eq!(reloaded.list()[0].b.label, "Builder");
        links.remove_node("a");
        assert!(Links::load_from(path.clone()).list().is_empty());
        let _ = std::fs::remove_file(&path);
    }
}
