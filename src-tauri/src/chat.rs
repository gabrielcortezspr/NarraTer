//! NarraTer Chat (Maestri Chat): the user talks to an agent from a chat panel
//! instead of its terminal. A message reaches the agent's terminal with a
//! header naming the thread; the agent answers with `narrater say` (each say
//! is a bubble), and `narrater recall` replays a thread after a restart or a
//! context compaction. Seven threads per terminal, named by color.
//!
//! Stored per terminal in `~/.config/narrater/chat/<terminal id>.json`;
//! attachments are copied into `~/.config/narrater/chat/attachments/`.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use chrono::{DateTime, Duration as ChronoDuration, Local, NaiveDate, TimeZone};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::{AppHandle, Emitter, State};

use crate::pty::{enqueue_message, now_ms, PtyState, PtyStateInner, QueuedMsg};

pub const THREADS: [&str; 7] = ["blue", "purple", "pink", "red", "orange", "yellow", "green"];
/// An identical reply this soon after the previous one is the same reply.
const DEDUP_MS: u64 = 10_000;
const IMAGE_EXTS: [&str; 6] = ["png", "jpg", "jpeg", "gif", "webp", "heic"];

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Attachment {
    pub id: String,
    pub name: String,
    /// Stored copy (absolute)
    pub path: String,
    #[serde(default)]
    pub image: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Entry {
    pub id: String,
    /// Epoch milliseconds
    pub ts: u64,
    /// "user" | "agent"
    pub role: String,
    /// Interim update (`say --progress`) — the turn goes on
    #[serde(default)]
    pub progress: bool,
    pub text: String,
    #[serde(default)]
    pub attachments: Vec<Attachment>,
}

#[derive(Serialize, Deserialize, Default)]
struct ChatFile {
    #[serde(default)]
    threads: BTreeMap<String, Vec<Entry>>,
}

#[derive(Default)]
pub struct ChatInner {
    /// Loaded chats by terminal id
    chats: HashMap<String, ChatFile>,
    /// Thread of the last message delivered to each terminal — where `say` goes
    active: HashMap<String, String>,
}

#[derive(Clone, Default)]
pub struct ChatState(pub Arc<Mutex<ChatInner>>);

#[derive(Serialize, Clone)]
struct ChangedEvent {
    terminal_id: String,
    thread: String,
    /// The agent spoke (the panel marks it unread when closed)
    from_agent: bool,
}

fn chat_dir() -> PathBuf {
    dirs::config_dir().unwrap_or_else(|| PathBuf::from(".")).join("narrater/chat")
}

fn chat_path(terminal: &str) -> PathBuf {
    let safe: String = terminal.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '-').collect();
    chat_dir().join(format!("{}.json", safe))
}

fn new_id() -> String {
    uuid::Uuid::new_v4().simple().to_string()[..10].to_string()
}

fn valid_thread(t: &str) -> Result<String, String> {
    let t = t.trim().to_lowercase();
    if THREADS.contains(&t.as_str()) {
        Ok(t)
    } else {
        Err(format!("unknown thread '{}' — threads: {}", t, THREADS.join(", ")))
    }
}

/// Copies a file into the attachment store.
fn store_attachment(src: &Path) -> Result<Attachment, String> {
    let meta = std::fs::metadata(src).map_err(|e| format!("cannot read {}: {}", src.display(), e))?;
    if !meta.is_file() {
        return Err(format!("{} is not a file", src.display()));
    }
    let id = new_id();
    let name = src.file_name().and_then(|n| n.to_str()).unwrap_or("file").to_string();
    let dir = chat_dir().join("attachments");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let dest = dir.join(format!("{}-{}", id, name));
    std::fs::copy(src, &dest).map_err(|e| format!("cannot store {}: {}", name, e))?;
    let image = is_image(src);
    Ok(Attachment { id, name, path: dest.display().to_string(), image })
}

fn is_image(p: &Path) -> bool {
    p.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| IMAGE_EXTS.contains(&e.to_lowercase().as_str()))
}

/// The prompt an agent receives for a chat message.
pub fn frame(thread: &str, text: &str, attachments: &[Attachment]) -> String {
    let mut out = format!(
        "---\nBelow is a message from the user through NarraTer Chat.\nReply only via `narrater say`. If the narrater-chat skill isn't already loaded, load it first.\nthread: {}\n---\n{}",
        thread,
        text.trim_end()
    );
    if !attachments.is_empty() {
        out.push_str("\n\nAttachments:");
        for a in attachments {
            out.push_str(&format!("\n- {} ({})", a.path, a.name));
        }
    }
    out
}

impl ChatState {
    fn with_chat<T>(&self, terminal: &str, f: impl FnOnce(&mut ChatFile) -> T) -> T {
        let mut inner = self.0.lock().unwrap();
        let chat = inner.chats.entry(terminal.to_string()).or_insert_with(|| {
            std::fs::read_to_string(chat_path(terminal))
                .ok()
                .and_then(|raw| serde_json::from_str(&raw).ok())
                .unwrap_or_default()
        });
        f(chat)
    }

    fn persist(&self, terminal: &str) {
        let inner = self.0.lock().unwrap();
        if let Some(chat) = inner.chats.get(terminal) {
            let _ = std::fs::create_dir_all(chat_dir());
            if let Ok(json) = serde_json::to_string_pretty(chat) {
                let _ = std::fs::write(chat_path(terminal), json);
            }
        }
    }

    fn append(&self, app: &AppHandle, terminal: &str, thread: &str, entry: Entry) {
        let from_agent = entry.role == "agent";
        self.with_chat(terminal, |c| c.threads.entry(thread.to_string()).or_default().push(entry));
        self.persist(terminal);
        let _ = app.emit("chat_changed", ChangedEvent { terminal_id: terminal.into(), thread: thread.into(), from_agent });
    }

    fn entries(&self, terminal: &str, thread: &str) -> Vec<Entry> {
        self.with_chat(terminal, |c| c.threads.get(thread).cloned().unwrap_or_default())
    }

    // ---- agent side (CLI) ----

    /// Where `say` goes: the thread of the last message delivered, or — after
    /// an app restart — the thread whose latest entry is the newest user message.
    fn active_thread(&self, terminal: &str) -> Option<String> {
        if let Some(t) = self.0.lock().unwrap().active.get(terminal).cloned() {
            return Some(t);
        }
        self.with_chat(terminal, |c| {
            c.threads
                .iter()
                .filter_map(|(t, e)| e.iter().rev().find(|x| x.role == "user").map(|x| (x.ts, t.clone())))
                .max()
                .map(|(_, t)| t)
        })
    }

    /// `narrater say` from terminal `from`.
    fn say(&self, app: &AppHandle, from: &str, p: &Value) -> Result<String, String> {
        let thread = self.active_thread(from).ok_or(
            "`say` answers NarraTer Chat messages, and this terminal hasn't received one — reply in the terminal instead",
        )?;
        let progress = p.get("progress").and_then(Value::as_bool).unwrap_or(false);
        let mut text = p.get("text").and_then(Value::as_str).unwrap_or("").to_string();
        let mut attachments = Vec::new();
        if let Some(file) = p.get("file").and_then(Value::as_str).filter(|f| !f.is_empty()) {
            let path = PathBuf::from(file);
            if is_image(&path) {
                attachments.push(store_attachment(&path)?);
            } else {
                let body = std::fs::read_to_string(&path).map_err(|e| format!("cannot read {} as UTF-8 text: {}", file, e))?;
                text = if text.is_empty() { body } else { format!("{}\n\n{}", text, body) };
            }
        }
        for f in p.get("attach").and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str) {
            if progress {
                return Err("--attach goes on the final reply, not on --progress".into());
            }
            attachments.push(store_attachment(Path::new(f))?);
        }
        // ![Description](/abs/path.png) on its own line presents an image inline
        let mut kept = Vec::new();
        for line in text.lines() {
            let t = line.trim();
            let inline = t.strip_prefix("![").and_then(|r| r.split_once("](")).and_then(|(_, rest)| rest.strip_suffix(')'));
            match inline.map(PathBuf::from).filter(|p| p.is_absolute() && p.is_file() && is_image(p)) {
                Some(path) => attachments.push(store_attachment(&path)?),
                None => kept.push(line),
            }
        }
        let text = kept.join("\n").trim().to_string();
        if text.is_empty() && attachments.is_empty() {
            return Err("nothing to say — pass text, a heredoc, --file or --attach".into());
        }
        let now = now_ms();
        let duplicate = self.entries(from, &thread).last().is_some_and(|last| {
            last.role == "agent" && last.text == text && attachments.is_empty() && now.saturating_sub(last.ts) < DEDUP_MS
        });
        if !duplicate {
            self.append(app, from, &thread, Entry { id: new_id(), ts: now, role: "agent".into(), progress, text, attachments });
        }
        Ok(if progress { "ok: progress shown".into() } else { "ok: sent".into() })
    }

    /// `narrater recall …` from terminal `from`.
    fn recall(&self, from: &str, p: &Value) -> Result<String, String> {
        let sub = p.get("thread").and_then(Value::as_str).unwrap_or("");
        if sub == "list" {
            return Ok(self.with_chat(from, |c| {
                THREADS
                    .iter()
                    .map(|t| match c.threads.get(*t).filter(|e| !e.is_empty()) {
                        Some(e) => format!("{:<7} {} message{}  last {}", t, e.len(), if e.len() == 1 { "" } else { "s" }, fmt_ts(e.last().unwrap().ts)),
                        None => format!("{:<7} (empty)", t),
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            }));
        }
        if sub == "attachment" {
            let id = p.get("id").and_then(Value::as_str).unwrap_or("");
            return self
                .with_chat(from, |c| c.threads.values().flatten().flat_map(|e| e.attachments.clone()).find(|a| a.id == id))
                .map(|a| format!("{}  ({})", a.path, a.name))
                .ok_or_else(|| format!("no attachment '{}' in this terminal's chat", id));
        }
        let thread = if sub.is_empty() {
            self.active_thread(from).ok_or("no current thread — name one: blue, purple, pink, red, orange, yellow, green")?
        } else {
            valid_thread(sub)?
        };
        let entries = filter_entries(self.entries(from, &thread), p, Local::now())?;
        if p.get("json").and_then(Value::as_bool).unwrap_or(false) {
            return serde_json::to_string_pretty(&entries).map_err(|e| e.to_string());
        }
        if entries.is_empty() {
            return Ok(format!("thread {} — nothing in this range", thread));
        }
        let mut out = vec![format!("thread {} — {} message{}", thread, entries.len(), if entries.len() == 1 { "" } else { "s" })];
        for e in &entries {
            let who = if e.role == "user" { "user" } else if e.progress { "you (progress)" } else { "you" };
            out.push(format!("[{}] {}: {}", fmt_ts(e.ts), who, e.text));
            for a in &e.attachments {
                out.push(format!("    attachment {}: {} ({})", a.id, a.name, a.path));
            }
        }
        Ok(out.join("\n"))
    }
}

fn fmt_ts(ms: u64) -> String {
    Local
        .timestamp_millis_opt(ms as i64)
        .single()
        .map(|t| t.format("%Y-%m-%d %H:%M").to_string())
        .unwrap_or_default()
}

/// `--turns N` (default 10) | `--last day|week|month` | `--from D [--to D]` | `--all`.
/// A turn is a user message and everything the agent said after it.
pub fn filter_entries(entries: Vec<Entry>, p: &Value, now: DateTime<Local>) -> Result<Vec<Entry>, String> {
    let get = |k: &str| p.get(k).and_then(Value::as_str).filter(|v| !v.is_empty());
    if p.get("all").and_then(Value::as_bool).unwrap_or(false) {
        return Ok(entries);
    }
    let day_start = |d: NaiveDate| Local.from_local_datetime(&d.and_hms_opt(0, 0, 0).unwrap()).earliest().map(|t| t.timestamp_millis() as u64);
    if let Some(last) = get("last") {
        let span = match last {
            "day" => ChronoDuration::days(1),
            "week" => ChronoDuration::weeks(1),
            "month" => ChronoDuration::days(30),
            _ => return Err("--last takes day, week or month".into()),
        };
        let since = (now - span).timestamp_millis() as u64;
        return Ok(entries.into_iter().filter(|e| e.ts >= since).collect());
    }
    if let Some(from) = get("from") {
        let parse = |s: &str| NaiveDate::parse_from_str(s, "%Y-%m-%d").map_err(|_| format!("dates are YYYY-MM-DD (got '{}')", s));
        let start = day_start(parse(from)?).unwrap_or(0);
        // --to includes that whole day
        let end = match get("to") {
            Some(to) => day_start(parse(to)? + ChronoDuration::days(1)).unwrap_or(u64::MAX),
            None => u64::MAX,
        };
        return Ok(entries.into_iter().filter(|e| e.ts >= start && e.ts < end).collect());
    }
    let turns = p.get("turns").and_then(Value::as_u64).unwrap_or(10);
    if !(1..=500).contains(&turns) {
        return Err("--turns must be between 1 and 500".into());
    }
    let starts: Vec<usize> = entries.iter().enumerate().filter(|(_, e)| e.role == "user").map(|(i, _)| i).collect();
    let cut = if starts.len() as u64 > turns { starts[starts.len() - turns as usize] } else { 0 };
    Ok(entries.into_iter().skip(cut).collect())
}

/// `narrater say` / `narrater recall` (mode "chat").
pub fn run_cli(chat: &ChatState, app: &AppHandle, from: &str, p: &Value) -> String {
    let result = match p.get("verb").and_then(Value::as_str).unwrap_or("") {
        "say" => chat.say(app, from, p),
        "recall" => chat.recall(from, p),
        _ => Err("unknown chat command".into()),
    };
    result.unwrap_or_else(|e| format!("Error: {}", e))
}

// ---- user side (chat panel) ----------------------------------------------

/// Sends a chat message to an agent terminal: stored, then typed into the
/// terminal (idle-gated, like `ask`) with the NarraTer Chat header.
#[tauri::command]
pub fn chat_send(
    terminal_id: String,
    thread: String,
    text: String,
    attachments: Vec<String>,
    app: AppHandle,
    chat: State<'_, ChatState>,
    pty: State<'_, PtyState>,
) -> Result<(), String> {
    let thread = valid_thread(&thread)?;
    if text.trim().is_empty() && attachments.is_empty() {
        return Err("empty message".into());
    }
    if !pty.0.lock().unwrap().sessions.contains_key(&terminal_id) {
        return Err("this terminal isn't running — restart it first".into());
    }
    let stored: Vec<Attachment> = attachments.iter().map(|p| store_attachment(Path::new(p))).collect::<Result<_, _>>()?;
    let prompt = frame(&thread, &text, &stored);
    chat.0.lock().unwrap().active.insert(terminal_id.clone(), thread.clone());
    chat.append(&app, &terminal_id, &thread, Entry {
        id: new_id(),
        ts: now_ms(),
        role: "user".into(),
        progress: false,
        text: text.trim_end().to_string(),
        attachments: stored,
    });
    deliver(&pty.0, &app, &terminal_id, prompt);
    Ok(())
}

fn deliver(pty: &Arc<Mutex<PtyStateInner>>, app: &AppHandle, terminal: &str, prompt: String) {
    enqueue_message(pty, app, terminal, QueuedMsg {
        from_label: "chat".into(),
        from_id: None,
        msg: prompt,
        delivered_tx: None,
    });
}

#[tauri::command]
pub fn chat_history(terminal_id: String, thread: String, chat: State<'_, ChatState>) -> Result<Vec<Entry>, String> {
    Ok(chat.entries(&terminal_id, &valid_thread(&thread)?))
}

#[derive(Serialize)]
pub struct ThreadSummary {
    thread: &'static str,
    count: usize,
    last_ts: Option<u64>,
}

#[tauri::command]
pub fn chat_threads(terminal_id: String, chat: State<'_, ChatState>) -> Vec<ThreadSummary> {
    chat.with_chat(&terminal_id, |c| {
        THREADS
            .iter()
            .map(|t| {
                let e = c.threads.get(*t);
                ThreadSummary { thread: t, count: e.map_or(0, Vec::len), last_ts: e.and_then(|e| e.last()).map(|e| e.ts) }
            })
            .collect()
    })
}

#[tauri::command]
pub fn chat_clear(terminal_id: String, thread: String, app: AppHandle, chat: State<'_, ChatState>) -> Result<(), String> {
    let thread = valid_thread(&thread)?;
    chat.with_chat(&terminal_id, |c| c.threads.remove(&thread));
    chat.persist(&terminal_id);
    let _ = app.emit("chat_changed", ChangedEvent { terminal_id, thread, from_agent: false });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn e(role: &str, ts: u64, text: &str) -> Entry {
        Entry { id: text.into(), ts, role: role.into(), progress: false, text: text.into(), attachments: vec![] }
    }

    #[test]
    fn frame_matches_the_chat_header() {
        let f = frame("blue", "hi there\n", &[]);
        assert!(f.starts_with("---\nBelow is a message from the user through NarraTer Chat.\nReply only via `narrater say`."));
        assert!(f.contains("\nthread: blue\n---\nhi there"));
        let a = Attachment { id: "x".into(), name: "a.png".into(), path: "/s/x-a.png".into(), image: true };
        assert!(frame("red", "look", &[a]).ends_with("Attachments:\n- /s/x-a.png (a.png)"));
    }

    #[test]
    fn threads_are_the_seven_colors() {
        assert_eq!(valid_thread(" Blue ").unwrap(), "blue");
        assert!(valid_thread("teal").is_err());
    }

    #[test]
    fn recall_filters() {
        // Noon today: "2 hours ago" and "5 minutes ago" stay on the same day
        let now = Local::now().date_naive().and_hms_opt(12, 0, 0).and_then(|n| Local.from_local_datetime(&n).earliest()).unwrap();
        let ms = |d: ChronoDuration| (now - d).timestamp_millis() as u64;
        let entries = vec![
            e("user", ms(ChronoDuration::days(40)), "old q"),
            e("agent", ms(ChronoDuration::days(40)), "old a"),
            e("user", ms(ChronoDuration::hours(2)), "q1"),
            e("agent", ms(ChronoDuration::hours(2)), "a1"),
            e("agent", ms(ChronoDuration::hours(2)), "a1b"),
            e("user", ms(ChronoDuration::minutes(5)), "q2"),
            e("agent", ms(ChronoDuration::minutes(5)), "a2"),
        ];
        let texts = |v: Vec<Entry>| v.into_iter().map(|e| e.text).collect::<Vec<_>>();
        assert_eq!(texts(filter_entries(entries.clone(), &json!({ "turns": 2 }), now).unwrap()), ["q1", "a1", "a1b", "q2", "a2"]);
        assert_eq!(texts(filter_entries(entries.clone(), &json!({ "turns": 1 }), now).unwrap()), ["q2", "a2"]);
        assert_eq!(filter_entries(entries.clone(), &json!({}), now).unwrap().len(), 7);
        assert_eq!(filter_entries(entries.clone(), &json!({ "last": "day" }), now).unwrap().len(), 5);
        assert_eq!(filter_entries(entries.clone(), &json!({ "all": true }), now).unwrap().len(), 7);
        let today = now.format("%Y-%m-%d").to_string();
        assert_eq!(filter_entries(entries.clone(), &json!({ "from": today, "to": today }), now).unwrap().len(), 5);
        assert!(filter_entries(entries.clone(), &json!({ "turns": 0 }), now).is_err());
        assert!(filter_entries(entries, &json!({ "last": "year" }), now).is_err());
    }
}
