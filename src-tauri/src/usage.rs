//! Agent usage on the canvas (docs/design.md, "Uso dos agentes"; D17).
//!
//! Claude Code pipes a JSON payload to its `statusLine` command after every
//! assistant message: the context window in use, the session cost and, for
//! Pro/Max subscribers, `rate_limits.five_hour` / `seven_day` (percentage +
//! reset). NarraTer installs a small status-line script that forwards that
//! payload to the app through `narrater usage-report`, then shows the
//! user's own status line if they had one. It is optional and never part of
//! the agent protocol (D01): without it the canvas just shows no usage.
//!
//! Installation rules for `~/.claude/settings.json` (the `statusLine` slot):
//! - empty slot, never installed → install ours;
//! - ours → keep (refresh the script);
//! - the user's own command → wrap it: our script runs it with the same
//!   payload, so their status line looks exactly the same;
//! - empty slot after we installed once → the user removed it: leave it.

use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use serde_json::Value;

const SCRIPT_NAME: &str = "claude-statusline.sh";

fn data_dir() -> PathBuf {
    dirs::data_dir().unwrap_or_else(|| PathBuf::from(".")).join("narrater")
}

fn script_path() -> PathBuf {
    data_dir().join(SCRIPT_NAME)
}

/// The user's own status-line command, run by ours after forwarding.
fn wrapped_path() -> PathBuf {
    data_dir().join("claude-statusline.user")
}

/// We installed once: an empty slot later means the user opted out.
fn marker_path() -> PathBuf {
    data_dir().join("claude-statusline.installed")
}

fn claude_settings_path() -> PathBuf {
    let base = std::env::var_os("CLAUDE_CONFIG_DIR")
        .map(PathBuf::from)
        .or_else(|| dirs::home_dir().map(|h| h.join(".claude")))
        .unwrap_or_else(|| PathBuf::from(".claude"));
    base.join("settings.json")
}

/// The status-line script. POSIX sh only; it must be quick and never fail
/// (Claude Code cancels it when the next update comes).
pub const SCRIPT: &str = r#"#!/bin/sh
# NarraTer — forwards Claude Code's status-line payload (context, cost, rate
# limits) to the canvas, then shows the user's own status line, if any.
# Outside a NarraTer terminal it only runs the user's command.
payload=$(cat)
if [ -n "$NARRATER_ID" ] && [ -n "$payload" ]; then
  stamp="${TMPDIR:-/tmp}/narrater-statusline-$NARRATER_ID"
  now=$(date +%s)
  last=$(cat "$stamp" 2>/dev/null || echo 0)
  case "$last" in *[!0-9]*|'') last=0 ;; esac
  if [ $((now - last)) -ge 2 ]; then
    echo "$now" > "$stamp" 2>/dev/null
    printf '%s' "$payload" | "${NARRATER_CLI:-narrater}" usage-report >/dev/null 2>&1
  fi
fi
user_cmd="__USER_CMD_FILE__"
if [ -s "$user_cmd" ]; then
  printf '%s' "$payload" | sh -c "$(cat "$user_cmd")"
fi
exit 0
"#;

#[derive(Debug, PartialEq)]
pub enum SlotAction {
    /// Nothing to change.
    Keep,
    /// Put our command in the slot.
    Install,
    /// Put our command in the slot and run this user command from it.
    Wrap(String),
    /// The user removed ours: respect it.
    OptedOut,
}

/// What to do with the `statusLine` slot, given its current command.
pub fn plan(current: Option<&str>, ours: &str, installed_before: bool) -> SlotAction {
    match current.map(str::trim).filter(|c| !c.is_empty()) {
        None if installed_before => SlotAction::OptedOut,
        None => SlotAction::Install,
        Some(c) if c.contains(SCRIPT_NAME) || c == ours => SlotAction::Keep,
        Some(c) => SlotAction::Wrap(c.to_string()),
    }
}

fn write_atomic(path: &std::path::Path, content: &str) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("narrater-tmp");
    std::fs::write(&tmp, content)?;
    std::fs::rename(&tmp, path)
}

/// Installs or refreshes the status-line forwarder. Never clobbers a
/// settings file it can't parse. Best effort: errors are logged, not fatal.
pub fn install_claude_statusline() {
    if let Err(e) = try_install() {
        eprintln!("[NarraTer usage] status line not installed: {}", e);
    }
}

fn try_install() -> Result<(), String> {
    let script = script_path();
    let body = SCRIPT.replace("__USER_CMD_FILE__", &wrapped_path().display().to_string());
    write_atomic(&script, &body).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755));
    }
    let ours = script.display().to_string();

    let settings_path = claude_settings_path();
    let mut root: Value = match std::fs::read_to_string(&settings_path) {
        Ok(raw) => serde_json::from_str(&raw).map_err(|_| "settings.json is not valid JSON — left untouched".to_string())?,
        Err(_) => serde_json::json!({}),
    };
    let Some(obj) = root.as_object_mut() else { return Err("settings.json is not an object".into()) };
    let current = obj.get("statusLine").and_then(|s| s.get("command")).and_then(Value::as_str).map(str::to_string);

    match plan(current.as_deref(), &ours, marker_path().exists()) {
        SlotAction::Keep | SlotAction::OptedOut => return Ok(()),
        SlotAction::Install => {
            obj.insert("statusLine".into(), serde_json::json!({ "type": "command", "command": ours }));
        }
        SlotAction::Wrap(user_cmd) => {
            write_atomic(&wrapped_path(), &user_cmd).map_err(|e| e.to_string())?;
            // Keep the user's other status-line options (padding, refreshInterval…)
            if let Some(sl) = obj.get_mut("statusLine").and_then(Value::as_object_mut) {
                sl.insert("command".into(), Value::String(ours));
            }
        }
    }
    let text = serde_json::to_string_pretty(&root).map_err(|e| e.to_string())? + "\n";
    write_atomic(&settings_path, &text).map_err(|e| e.to_string())?;
    let _ = write_atomic(&marker_path(), "");
    Ok(())
}

/// One rate-limit window: percentage used and when it resets.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct UsageWindow {
    pub used_percent: f64,
    /// Unix epoch seconds.
    pub resets_at: Option<i64>,
}

/// Account-level limits of one provider (all its terminals share them).
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ProviderUsage {
    pub provider: String,
    pub five_hour: Option<UsageWindow>,
    pub seven_day: Option<UsageWindow>,
    /// Epoch milliseconds of the report.
    pub updated: u64,
}

/// What one agent session reported about itself.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct SessionUsage {
    pub terminal: String,
    pub context_percent: Option<f64>,
    pub cost_usd: Option<f64>,
    pub model: Option<String>,
    pub updated: u64,
}

#[derive(Serialize, Clone, Debug)]
pub struct UsageEvent {
    pub provider: Option<ProviderUsage>,
    pub session: SessionUsage,
}

#[derive(Default)]
pub struct UsageState {
    pub providers: HashMap<String, ProviderUsage>,
    pub sessions: HashMap<String, SessionUsage>,
    /// Writes provider limits to disk (off in tests)
    persist: bool,
}

fn usage_path() -> PathBuf {
    dirs::config_dir().unwrap_or_else(|| PathBuf::from(".")).join("narrater/usage.json")
}

impl UsageState {
    /// Provider limits survive a restart (shown with their age); sessions don't.
    pub fn load() -> Self {
        let providers = std::fs::read_to_string(usage_path())
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default();
        UsageState { providers, sessions: HashMap::new(), persist: true }
    }

    fn save(&self) {
        if !self.persist {
            return;
        }
        if let Ok(text) = serde_json::to_string_pretty(&self.providers) {
            let _ = write_atomic(&usage_path(), &text);
        }
    }

    /// Records a Claude Code status-line payload sent by `terminal`.
    pub fn ingest_claude(&mut self, terminal: &str, payload: &Value, now_ms: u64) -> UsageEvent {
        let num = |v: &Value, path: &[&str]| -> Option<f64> {
            path.iter().try_fold(v, |acc, k| acc.get(*k)).and_then(Value::as_f64).filter(|x| x.is_finite())
        };
        let window = |name: &str| -> Option<UsageWindow> {
            let w = payload.get("rate_limits")?.get(name)?;
            let used = w.get("used_percentage").and_then(Value::as_f64).filter(|x| x.is_finite())?;
            let resets_at = w.get("resets_at").and_then(|r| r.as_i64().or_else(|| r.as_f64().map(|f| f as i64)));
            Some(UsageWindow { used_percent: used.clamp(0.0, 100.0), resets_at })
        };
        let session = SessionUsage {
            terminal: terminal.to_string(),
            context_percent: num(payload, &["context_window", "used_percentage"]),
            cost_usd: num(payload, &["cost", "total_cost_usd"]),
            model: payload.get("model").and_then(|m| m.get("display_name")).and_then(Value::as_str).map(str::to_string),
            updated: now_ms,
        };
        self.sessions.insert(terminal.to_string(), session.clone());

        let (five, seven) = (window("five_hour"), window("seven_day"));
        let provider = if five.is_some() || seven.is_some() {
            let p = ProviderUsage { provider: "claude".into(), five_hour: five, seven_day: seven, updated: now_ms };
            self.providers.insert("claude".into(), p.clone());
            self.save();
            Some(p)
        } else {
            None
        };
        UsageEvent { provider, session }
    }
}

#[derive(Serialize)]
pub struct UsageSnapshot {
    pub providers: Vec<ProviderUsage>,
    pub sessions: Vec<SessionUsage>,
}

/// Frontend start-up: everything known so far.
#[tauri::command]
pub fn usage_snapshot(state: tauri::State<'_, crate::pty::PtyState>) -> UsageSnapshot {
    let usage = std::sync::Arc::clone(&state.0.lock().unwrap().usage);
    let u = usage.lock().unwrap();
    UsageSnapshot {
        providers: u.providers.values().cloned().collect(),
        sessions: u.sessions.values().cloned().collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_slot_is_never_clobbered() {
        let ours = "/home/u/.local/share/narrater/claude-statusline.sh";
        assert_eq!(plan(None, ours, false), SlotAction::Install);
        assert_eq!(plan(Some("  "), ours, false), SlotAction::Install);
        assert_eq!(plan(None, ours, true), SlotAction::OptedOut);
        assert_eq!(plan(Some(ours), ours, true), SlotAction::Keep);
        assert_eq!(plan(Some("bash /other/home/claude-statusline.sh"), ours, false), SlotAction::Keep);
        assert_eq!(plan(Some("~/bin/my-line.sh"), ours, false), SlotAction::Wrap("~/bin/my-line.sh".into()));
    }

    #[test]
    fn ingests_the_documented_payload() {
        // Shape from code.claude.com/docs/en/statusline ("Available data")
        let payload = serde_json::json!({
            "model": { "id": "claude-opus", "display_name": "Opus" },
            "cost": { "total_cost_usd": 0.42 },
            "context_window": { "used_percentage": 37.5 },
            "rate_limits": {
                "five_hour": { "used_percentage": 23.5, "resets_at": 1738425600 },
                "seven_day": { "used_percentage": 41.2, "resets_at": 1738857600 }
            }
        });
        let mut s = UsageState::default();
        let ev = s.ingest_claude("t1", &payload, 1000);
        let p = ev.provider.unwrap();
        assert_eq!(p.five_hour, Some(UsageWindow { used_percent: 23.5, resets_at: Some(1738425600) }));
        assert_eq!(p.seven_day.unwrap().used_percent, 41.2);
        assert_eq!(ev.session.context_percent, Some(37.5));
        assert_eq!(ev.session.cost_usd, Some(0.42));
        assert_eq!(ev.session.model.as_deref(), Some("Opus"));
    }

    #[test]
    fn api_key_sessions_have_no_limits_but_keep_the_context() {
        // rate_limits only exists for Pro/Max subscribers (same doc)
        let payload = serde_json::json!({ "context_window": { "used_percentage": null }, "cost": { "total_cost_usd": 0.01 } });
        let mut s = UsageState::default();
        let ev = s.ingest_claude("t1", &payload, 1);
        assert!(ev.provider.is_none());
        assert_eq!(ev.session.context_percent, None);
        assert_eq!(ev.session.cost_usd, Some(0.01));
        assert!(s.providers.is_empty());
    }
}
