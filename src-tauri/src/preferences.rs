//! App preferences (`~/.config/narrater/preferences.json`), read by the
//! backend and edited from the Preferences sheet.

use std::path::PathBuf;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

fn yes() -> bool {
    true
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Preferences {
    /// Local terminals run inside tmux (when installed), so agents keep
    /// running when NarraTer closes and reattach when it opens again
    #[serde(default = "yes")]
    pub persistent_sessions: bool,
    /// Automatic teams: any agent may build a team with `narrater team create`
    /// when it judges the task worth it (off: only a Maestro terminal, and only
    /// when the user asks). Off by default: one agent is the default (D22).
    #[serde(default)]
    pub auto_teams: bool,
}

impl Default for Preferences {
    fn default() -> Self {
        Preferences { persistent_sessions: true, auto_teams: false }
    }
}

fn path() -> PathBuf {
    dirs::config_dir().unwrap_or_else(|| PathBuf::from(".")).join("narrater/preferences.json")
}

static CACHE: Mutex<Option<Preferences>> = Mutex::new(None);

pub fn get() -> Preferences {
    let mut cache = CACHE.lock().unwrap();
    cache
        .get_or_insert_with(|| {
            std::fs::read_to_string(path()).ok().and_then(|raw| serde_json::from_str(&raw).ok()).unwrap_or_default()
        })
        .clone()
}

#[tauri::command]
pub fn preferences_get() -> Preferences {
    get()
}

#[tauri::command]
pub fn preferences_set(preferences: Preferences) -> Result<(), String> {
    let p = path();
    if let Some(dir) = p.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let text = serde_json::to_string_pretty(&preferences).map_err(|e| e.to_string())?;
    let tmp = p.with_extension("json.tmp");
    std::fs::write(&tmp, text).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, &p).map_err(|e| e.to_string())?;
    *CACHE.lock().unwrap() = Some(preferences);
    Ok(())
}
