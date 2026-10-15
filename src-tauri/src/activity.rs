//! Activity log: every state change of the agents and of the conversations
//! between them, written by the backend the moment it happens (never by an
//! agent). Feeds the Activity panel and survives restarts.
//!
//! One JSON line per event in `~/.config/narrater/activity.jsonl`; the file
//! rotates to `activity.1.jsonl` past ROTATE_BYTES. The newest KEEP events
//! stay in memory for the panel.

use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use tauri::{AppHandle, Emitter};

const KEEP: usize = 5000;
const ROTATE_BYTES: u64 = 5 * 1024 * 1024;
/// Prompts and answers are kept whole up to this; the full text of an ask
/// stays in asks.jsonl.
const TEXT_CAP: usize = 4000;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ActivityEvent {
    /// Epoch milliseconds.
    pub ts: u64,
    /// "state" | "ask" | "delivered" | "answer" | "message" | "spawn" | "exit" | "note"
    pub kind: String,
    /// The terminal the event is about.
    pub terminal: String,
    pub label: String,
    /// The other side (asker, target, sender), when there is one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub peer: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub peer_label: Option<String>,
    /// state: working | idle | attention · answer: done | attention | answered | timeout | cancelled | exited | error
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ask: Option<String>,
    /// Prompt or answer text (capped).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// Where an answer came from ("transcript" | "screen"), how long it took…
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

impl ActivityEvent {
    pub fn new(kind: &str, terminal: &str, label: &str) -> Self {
        ActivityEvent {
            ts: crate::pty::now_ms(),
            kind: kind.into(),
            terminal: terminal.into(),
            label: label.into(),
            peer: None,
            peer_label: None,
            status: None,
            ask: None,
            text: None,
            detail: None,
        }
    }
    pub fn peer(mut self, id: &str, label: &str) -> Self {
        self.peer = Some(id.into());
        self.peer_label = Some(label.into());
        self
    }
    pub fn status(mut self, s: &str) -> Self {
        self.status = Some(s.into());
        self
    }
    pub fn ask(mut self, id: &str) -> Self {
        self.ask = Some(id.into());
        self
    }
    pub fn text(mut self, t: &str) -> Self {
        self.text = Some(cap(t));
        self
    }
    pub fn detail(mut self, d: impl Into<String>) -> Self {
        self.detail = Some(d.into());
        self
    }
}

fn cap(t: &str) -> String {
    let n = t.chars().count();
    if n <= TEXT_CAP {
        return t.to_string();
    }
    let head: String = t.chars().take(TEXT_CAP).collect();
    format!("{}… [+{} chars]", head, n - TEXT_CAP)
}

struct Log {
    path: Option<PathBuf>,
    recent: VecDeque<ActivityEvent>,
    app: Option<AppHandle>,
}

fn log() -> &'static Mutex<Log> {
    static LOG: OnceLock<Mutex<Log>> = OnceLock::new();
    LOG.get_or_init(|| {
        // Tests never touch the user's log
        let path = if cfg!(test) { None } else { dirs::config_dir().map(|d| d.join("narrater").join("activity.jsonl")) };
        let recent = path.as_ref().map(|p| load_tail(p)).unwrap_or_default();
        Mutex::new(Log { path, recent, app: None })
    })
}

/// The newest KEEP events of the file (and of the rotated one, if short).
fn load_tail(path: &std::path::Path) -> VecDeque<ActivityEvent> {
    let mut out: VecDeque<ActivityEvent> = VecDeque::new();
    for p in [path.with_file_name("activity.1.jsonl"), path.to_path_buf()] {
        if let Ok(body) = std::fs::read_to_string(&p) {
            for line in body.lines() {
                if let Ok(ev) = serde_json::from_str::<ActivityEvent>(line) {
                    out.push_back(ev);
                    if out.len() > KEEP {
                        out.pop_front();
                    }
                }
            }
        }
    }
    out
}

/// Called once at startup so events reach the panel live.
pub fn attach(app: &AppHandle) {
    log().lock().unwrap().app = Some(app.clone());
}

/// Records an event: appends it to the file, keeps it for the panel and
/// emits `agent_activity`.
pub fn record(ev: ActivityEvent) {
    let app = {
        let mut l = log().lock().unwrap();
        if let Some(path) = l.path.clone() {
            if let Some(dir) = path.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            if std::fs::metadata(&path).map(|m| m.len() > ROTATE_BYTES).unwrap_or(false) {
                let _ = std::fs::rename(&path, path.with_file_name("activity.1.jsonl"));
            }
            if let (Ok(mut f), Ok(line)) =
                (std::fs::OpenOptions::new().create(true).append(true).open(&path), serde_json::to_string(&ev))
            {
                let _ = writeln!(f, "{}", line);
            }
        }
        l.recent.push_back(ev.clone());
        if l.recent.len() > KEEP {
            l.recent.pop_front();
        }
        l.app.clone()
    };
    if let Some(app) = app {
        let _ = app.emit("agent_activity", &ev);
    }
}

/// The newest `limit` events (oldest first), optionally for one terminal.
#[tauri::command]
pub fn activity_list(limit: Option<usize>, terminal: Option<String>) -> Vec<ActivityEvent> {
    let l = log().lock().unwrap();
    let limit = limit.unwrap_or(1000).min(KEEP);
    let matching: Vec<&ActivityEvent> = l
        .recent
        .iter()
        .filter(|e| terminal.as_deref().is_none_or(|t| e.terminal == t || e.peer.as_deref() == Some(t)))
        .collect();
    matching[matching.len().saturating_sub(limit)..].iter().map(|e| (*e).clone()).collect()
}

/// Where each running ask's answer came from ("transcript" | "screen"), set
/// by `run_ask` and read once when the ask is recorded as finished.
fn sources() -> &'static Mutex<std::collections::HashMap<String, &'static str>> {
    static S: OnceLock<Mutex<std::collections::HashMap<String, &'static str>>> = OnceLock::new();
    S.get_or_init(Default::default)
}

pub fn set_source(ask: &str, source: &'static str) {
    sources().lock().unwrap().insert(ask.to_string(), source);
}

pub fn take_source(ask: &str) -> Option<&'static str> {
    sources().lock().unwrap().remove(ask)
}

/// Empties the log (the panel's "Clear").
#[tauri::command]
pub fn activity_clear() {
    let mut l = log().lock().unwrap();
    l.recent.clear();
    if let Some(path) = &l.path {
        let _ = std::fs::remove_file(path);
        let _ = std::fs::remove_file(path.with_file_name("activity.1.jsonl"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn long_text_is_capped_with_a_count() {
        let t = "x".repeat(TEXT_CAP + 5);
        let c = cap(&t);
        assert!(c.ends_with("… [+5 chars]"));
    }

    #[test]
    fn events_round_trip_without_empty_fields() {
        let ev = ActivityEvent::new("state", "t1", "DevA").status("idle");
        let line = serde_json::to_string(&ev).unwrap();
        assert!(!line.contains("peer"));
        assert_eq!(serde_json::from_str::<ActivityEvent>(&line).unwrap(), ev);
    }
}
