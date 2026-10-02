//! Routines: scheduled prompts/commands fired into a terminal of the scene,
//! or desktop reminders (Maestri's maestri-routines). The scheduler lives in
//! the backend so it keeps time whatever the window is doing; delivery goes
//! through the same idle-gated queue as `narrater ask`.
//!
//! Routines belong to a scene: `~/.config/narrater/routines/<scene>.json`.

pub mod schedule;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use chrono::{DateTime, Local, TimeZone};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::{AppHandle, Emitter, State};

use crate::pty::{enqueue_message, PtyState, PtyStateInner, QueuedMsg};
use crate::screen::TurnState;
use schedule::Schedule;

const TICK: Duration = Duration::from_secs(1);
const PRE_RUN_TIMEOUT: Duration = Duration::from_secs(120);
const PRE_RUN_OUTPUT_CAP: usize = 64 * 1024;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum RoutineTarget {
    /// A terminal of the scene (label kept for display when it's not running).
    Terminal { id: String, label: String },
    /// No terminal: a desktop notification with the command text.
    Reminder,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Routine {
    pub id: String,
    pub name: String,
    pub command: String,
    pub schedule: Schedule,
    pub target: RoutineTarget,
    #[serde(default)]
    pub count: Option<u32>,
    #[serde(default)]
    pub until: Option<String>,
    #[serde(default)]
    pub pre_run: Option<String>,
    #[serde(default = "yes")]
    pub skip_if_busy: bool,
    #[serde(default = "yes")]
    pub notify: bool,
    #[serde(default = "yes")]
    pub enabled: bool,
    /// Ran its course (count, until, one-shot) — stays listed, paused.
    #[serde(default)]
    pub finished: bool,
    #[serde(default)]
    pub fired: u32,
    #[serde(default)]
    pub skipped: u32,
    /// Epoch seconds.
    #[serde(default)]
    pub last_fired: Option<i64>,
    #[serde(default)]
    pub next_fire: Option<i64>,
    #[serde(default)]
    pub last_result: Option<String>,
}

fn yes() -> bool {
    true
}

impl Routine {
    fn short_id(&self) -> &str {
        &self.id[..6.min(self.id.len())]
    }
}

#[derive(Default)]
pub struct RoutinesInner {
    scene: Option<String>,
    routines: Vec<Routine>,
}

#[derive(Clone, Default)]
pub struct RoutinesState(pub Arc<Mutex<RoutinesInner>>);

/// What the UI shows for a routine.
#[derive(Serialize, Clone)]
pub struct RoutineView {
    id: String,
    short_id: String,
    name: String,
    command: String,
    schedule: String,
    /// Terminal node id (None for reminders).
    target_id: Option<String>,
    target_label: String,
    status: &'static str,
    next_fire: Option<String>,
    fired: u32,
    skipped: u32,
    last_result: Option<String>,
}

#[derive(Serialize, Clone)]
struct FiredEvent {
    name: String,
    target: String,
    outcome: String,
    /// Show a toast (the routine's --no-notify turns it off).
    notify: bool,
}

fn routines_dir() -> PathBuf {
    dirs::config_dir().unwrap_or_else(|| PathBuf::from(".")).join("narrater").join("routines")
}

fn routines_path(scene: &str) -> PathBuf {
    routines_dir().join(format!("{}.json", scene))
}

fn fmt_time(epoch: i64) -> String {
    Local
        .timestamp_opt(epoch, 0)
        .single()
        .map(|t| t.format("%Y-%m-%d %H:%M:%S").to_string())
        .unwrap_or_default()
}

fn status_of(r: &Routine) -> &'static str {
    if r.finished {
        "finished"
    } else if r.enabled {
        "enabled"
    } else {
        "paused"
    }
}

fn target_label(r: &Routine) -> String {
    match &r.target {
        RoutineTarget::Terminal { label, .. } => label.clone(),
        RoutineTarget::Reminder => "reminder".into(),
    }
}

fn view(r: &Routine) -> RoutineView {
    RoutineView {
        id: r.id.clone(),
        short_id: r.short_id().to_string(),
        name: r.name.clone(),
        command: r.command.clone(),
        schedule: schedule::describe(&r.schedule),
        target_id: match &r.target {
            RoutineTarget::Terminal { id, .. } => Some(id.clone()),
            RoutineTarget::Reminder => None,
        },
        target_label: target_label(r),
        status: status_of(r),
        next_fire: r.next_fire.filter(|_| r.enabled).map(fmt_time),
        fired: r.fired,
        skipped: r.skipped,
        last_result: r.last_result.clone(),
    }
}

/// Next fire for a routine being (re)armed now. A one-shot missed while the
/// app was closed fires right away; recurring ones don't replay the past.
fn arm(r: &mut Routine, now: DateTime<Local>) {
    if !r.enabled || r.finished {
        r.next_fire = None;
        return;
    }
    if r.until.as_deref().is_some_and(|u| schedule::past_until(u, now)) {
        finish(r);
        return;
    }
    r.next_fire = match &r.schedule {
        Schedule::Once { at } if r.fired == 0 => {
            let at = chrono::NaiveDateTime::parse_from_str(at, "%Y-%m-%d %H:%M")
                .ok()
                .and_then(|n| Local.from_local_datetime(&n).earliest());
            Some(at.map_or(now, |t| t.max(now)).timestamp())
        }
        s => schedule::next_after(s, now).map(|t| t.timestamp()),
    };
    if r.next_fire.is_none() {
        finish(r);
    }
}

fn finish(r: &mut Routine) {
    r.enabled = false;
    r.finished = true;
    r.next_fire = None;
}

impl RoutinesState {
    /// The scene the app has open (set when it loads its routines).
    pub fn current_scene(&self) -> Option<String> {
        self.0.lock().unwrap().scene.clone()
    }

    fn save(&self) {
        let inner = self.0.lock().unwrap();
        let Some(scene) = &inner.scene else { return };
        let _ = std::fs::create_dir_all(routines_dir());
        let json = serde_json::json!({ "schemaVersion": 1, "routines": inner.routines });
        if let Ok(text) = serde_json::to_string_pretty(&json) {
            if let Err(e) = std::fs::write(routines_path(scene), text) {
                eprintln!("[NarraTer] could not save routines: {}", e);
            }
        }
    }

    fn views(&self) -> Vec<RoutineView> {
        self.0.lock().unwrap().routines.iter().map(view).collect()
    }

    fn changed(&self, app: &AppHandle) {
        self.save();
        let _ = app.emit("routines_changed", self.views());
    }
}

// ---- Firing --------------------------------------------------------------

/// Runs the pre-run script; its stdout replaces `{{output}}`. A non-zero exit
/// skips the fire.
async fn pre_run(script: &str, cwd: PathBuf, env: &[(&str, String)]) -> Result<String, String> {
    let mut cmd = tokio::process::Command::new("sh");
    cmd.arg("-c").arg(script).current_dir(cwd).kill_on_drop(true);
    for (k, v) in env {
        cmd.env(k, v);
    }
    let out = tokio::time::timeout(PRE_RUN_TIMEOUT, cmd.output())
        .await
        .map_err(|_| format!("pre-run timed out after {}s", PRE_RUN_TIMEOUT.as_secs()))?
        .map_err(|e| format!("pre-run could not start: {}", e))?;
    if !out.status.success() {
        let code = out.status.code().map_or("a signal".to_string(), |c| c.to_string());
        let err = String::from_utf8_lossy(&out.stderr);
        let first = err.lines().next().unwrap_or("").trim();
        return Err(format!("pre-run exited with {}{}", code, if first.is_empty() { String::new() } else { format!(": {}", first) }));
    }
    let mut text = String::from_utf8_lossy(&out.stdout).trim_end().to_string();
    if text.len() > PRE_RUN_OUTPUT_CAP {
        let mut cut = PRE_RUN_OUTPUT_CAP;
        while !text.is_char_boundary(cut) {
            cut -= 1;
        }
        text.truncate(cut);
        text.push_str("\n… (pre-run output truncated)");
    }
    Ok(text)
}

/// Fires routine `id` now. `manual` (`routine run`) queues even into a busy
/// terminal. Returns the outcome line.
async fn fire(routines: RoutinesState, pty: Arc<Mutex<PtyStateInner>>, app: AppHandle, id: String, manual: bool) -> String {
    let Some(r) = routines.0.lock().unwrap().routines.iter().find(|r| r.id == id).cloned() else {
        return "Error: routine not found".into();
    };
    let scene = routines.0.lock().unwrap().scene.clone().unwrap_or_default();

    let outcome: Result<String, String> = async {
        // Target state and working directory
        let (cwd, label) = match &r.target {
            RoutineTarget::Terminal { id: tid, label } => {
                let inner = pty.lock().unwrap();
                let Some(s) = inner.sessions.get(tid) else {
                    return Err(format!("'{}' is not running", label));
                };
                if r.skip_if_busy && !manual && s.turn != TurnState::Idle {
                    return Err(format!("'{}' was busy", inner.label_of(tid)));
                }
                (s.project_dir.clone(), inner.label_of(tid))
            }
            RoutineTarget::Reminder => (dirs::home_dir().unwrap_or_else(|| PathBuf::from("/")), "reminder".into()),
        };

        let mut text = r.command.clone();
        if let Some(script) = r.pre_run.as_deref().filter(|s| !s.trim().is_empty()) {
            let env = [
                ("NARRATER_SCENE_NAME", scene.clone()),
                ("NARRATER_SCENE_DIR", cwd.display().to_string()),
                ("NARRATER_ROUTINE_NAME", r.name.clone()),
                ("NARRATER_TERMINAL_NAME", label.clone()),
            ];
            let output = pre_run(script, cwd, &env).await?;
            text = text.replace("{{output}}", &output);
        }

        match &r.target {
            RoutineTarget::Terminal { id: tid, .. } => {
                enqueue_message(&pty, &app, tid, QueuedMsg {
                    from_label: format!("routine '{}'", r.name),
                    from_id: None,
                    msg: text,
                    delivered_tx: None,
                });
                Ok(format!("fired into '{}'", label))
            }
            RoutineTarget::Reminder => {
                let _ = std::process::Command::new("notify-send")
                    .args(["--app-name=NarraTer", "--", &format!("NarraTer · {}", r.name), &text])
                    .spawn();
                Ok("reminder shown".into())
            }
        }
    }
    .await;

    let now = Local::now();
    let line = {
        let mut inner = routines.0.lock().unwrap();
        let Some(rec) = inner.routines.iter_mut().find(|x| x.id == id) else {
            return "Error: routine was deleted meanwhile".into();
        };
        let line = match &outcome {
            Ok(done) => {
                rec.fired += 1;
                rec.last_fired = Some(now.timestamp());
                rec.last_result = Some(format!("{} at {}", done, now.format("%Y-%m-%d %H:%M:%S")));
                let once = matches!(rec.schedule, Schedule::Once { .. });
                if once || rec.count.is_some_and(|c| rec.fired >= c) {
                    finish(rec);
                }
                format!("ok: '{}' {}", rec.name, done)
            }
            Err(why) => {
                rec.skipped += 1;
                rec.last_result = Some(format!("skipped: {} at {}", why, now.format("%Y-%m-%d %H:%M:%S")));
                format!("skipped: '{}' — {}", rec.name, why)
            }
        };
        let _ = app.emit("routine_fired", FiredEvent {
            name: rec.name.clone(),
            target: target_label(rec),
            outcome: line.clone(),
            notify: rec.notify,
        });
        line
    };
    routines.changed(&app);
    line
}

/// Background scheduler: fires due routines of the loaded scene.
pub async fn start_scheduler(app: AppHandle, routines: RoutinesState, pty: Arc<Mutex<PtyStateInner>>) {
    let mut interval = tokio::time::interval(TICK);
    loop {
        interval.tick().await;
        let now = Local::now();
        let due: Vec<String> = {
            let mut inner = routines.0.lock().unwrap();
            let mut due = Vec::new();
            for r in inner.routines.iter_mut().filter(|r| r.enabled) {
                if r.until.as_deref().is_some_and(|u| schedule::past_until(u, now)) {
                    finish(r);
                    continue;
                }
                if r.next_fire.is_some_and(|t| t <= now.timestamp()) {
                    // Re-arm before firing: a slow pre-run must not double-fire
                    r.next_fire = match &r.schedule {
                        Schedule::Once { .. } => None,
                        s => schedule::next_after(s, now).map(|t| t.timestamp()),
                    };
                    due.push(r.id.clone());
                }
            }
            due
        };
        for id in due {
            tokio::spawn(fire(routines.clone(), Arc::clone(&pty), app.clone(), id, false));
        }
    }
}

// ---- CLI (`narrater routine …`, Maestro only) -----------------------------

fn s<'a>(p: &'a Value, k: &str) -> Option<&'a str> {
    p.get(k).and_then(Value::as_str).map(str::trim).filter(|v| !v.is_empty())
}

fn b(p: &Value, k: &str) -> bool {
    p.get(k).and_then(Value::as_bool).unwrap_or(false)
}

/// The one schedule flag given (None if none; error if several or invalid).
fn schedule_from(p: &Value, now: DateTime<Local>) -> Result<Option<Schedule>, String> {
    let given: Vec<&str> = ["every", "daily", "weekly", "once"].into_iter().filter(|k| s(p, k).is_some()).collect();
    if given.len() > 1 {
        return Err("pick exactly one schedule: --every, --daily, --weekly or --once".into());
    }
    Ok(match given.first().copied() {
        Some("every") => Some(Schedule::Every { secs: schedule::parse_every(s(p, "every").unwrap())? }),
        Some("daily") => {
            let (hour, minute) = schedule::parse_hm(s(p, "daily").unwrap())?;
            Some(Schedule::Daily { hour, minute })
        }
        Some("weekly") => Some(schedule::parse_weekly(s(p, "weekly").unwrap())?),
        Some("once") => Some(schedule::parse_once(s(p, "once").unwrap(), now)?),
        _ => None,
    })
}

fn resolve_terminal(pty: &PtyStateInner, name: &str) -> Result<(String, String), String> {
    let id = pty
        .label_to_id
        .get(name)
        .cloned()
        .or_else(|| {
            let lower = name.to_lowercase();
            let hits: Vec<&String> =
                pty.label_to_id.iter().filter(|(l, _)| l.to_lowercase() == lower).map(|(_, id)| id).collect();
            (hits.len() == 1).then(|| hits[0].clone())
        })
        .ok_or_else(|| format!("no running terminal named '{}' in this scene", name))?;
    Ok((id.clone(), pty.label_of(&id)))
}

fn target_from(p: &Value, pty: &PtyStateInner, from: &str) -> Result<Option<RoutineTarget>, String> {
    if b(p, "reminder") {
        if s(p, "terminal").is_some() {
            return Err("--reminder and --terminal don't go together".into());
        }
        return Ok(Some(RoutineTarget::Reminder));
    }
    if let Some(name) = s(p, "terminal") {
        let (id, label) = resolve_terminal(pty, name)?;
        return Ok(Some(RoutineTarget::Terminal { id, label }));
    }
    let _ = from;
    Ok(None)
}

/// Routines matching `name` (and `--id` prefix). Ambiguity lists the ids.
fn select<'a>(routines: &'a mut [Routine], p: &Value) -> Result<&'a mut Routine, String> {
    let name = s(p, "name");
    let id = s(p, "id");
    let hits: Vec<usize> = routines
        .iter()
        .enumerate()
        .filter(|(_, r)| name.map_or(true, |n| r.name.eq_ignore_ascii_case(n)))
        .filter(|(_, r)| id.map_or(true, |i| r.id.starts_with(i)))
        .map(|(i, _)| i)
        .collect();
    match hits.as_slice() {
        [i] => Ok(&mut routines[*i]),
        [] => Err(format!(
            "no routine {} — run `narrater routine list`",
            name.map(|n| format!("named '{}'", n)).or(id.map(|i| format!("with id {}", i))).unwrap_or_default()
        )),
        many => Err(format!(
            "several routines match — re-run with --id <prefix>:\n{}",
            many.iter().map(|i| format!("  {}  {}", routines[*i].short_id(), routines[*i].name)).collect::<Vec<_>>().join("\n")
        )),
    }
}

fn line(r: &Routine) -> String {
    format!(
        "{}  {}  {}  → {}  {}{}  fired {}{}",
        r.short_id(),
        r.name,
        schedule::describe(&r.schedule),
        target_label(r),
        status_of(r),
        r.next_fire.filter(|_| r.enabled).map(|t| format!("  next {}", fmt_time(t))).unwrap_or_default(),
        r.fired,
        if r.skipped > 0 { format!(" (skipped {})", r.skipped) } else { String::new() },
    )
}

fn detail(r: &Routine) -> String {
    let mut out = vec![
        format!("name:       {}", r.name),
        format!("id:         {}", r.id),
        format!("schedule:   {}", schedule::describe(&r.schedule)),
        format!("target:     {}", target_label(r)),
        format!("status:     {}", status_of(r)),
        format!("command:    {}", r.command),
    ];
    if let Some(p) = &r.pre_run {
        out.push(format!("pre-run:    {}", p));
    }
    if let Some(c) = r.count {
        out.push(format!("count:      stops after {} fires", c));
    }
    if let Some(u) = &r.until {
        out.push(format!("until:      {}", u));
    }
    out.push(format!("skip if busy: {}   notify: {}", if r.skip_if_busy { "yes" } else { "no" }, if r.notify { "yes" } else { "no" }));
    if let Some(t) = r.next_fire.filter(|_| r.enabled) {
        out.push(format!("next fire:  {}", fmt_time(t)));
    }
    out.push(format!("fired:      {} (skipped {})", r.fired, r.skipped));
    if let Some(l) = &r.last_result {
        out.push(format!("last:       {}", l));
    }
    out.join("\n")
}

/// Applies the optional fields shared by create and edit.
fn apply_options(r: &mut Routine, p: &Value) -> Result<(), String> {
    if let Some(c) = p.get("count").and_then(Value::as_u64) {
        if c == 0 {
            return Err("--count must be at least 1".into());
        }
        r.count = Some(c as u32);
    }
    if let Some(u) = s(p, "until") {
        r.until = Some(schedule::parse_until(u)?);
    }
    if let Some(script) = p.get("pre_run").and_then(Value::as_str) {
        r.pre_run = Some(script.to_string()).filter(|x| !x.trim().is_empty());
    }
    if b(p, "no_skip_if_busy") {
        r.skip_if_busy = false;
    }
    if b(p, "skip_if_busy") {
        r.skip_if_busy = true;
    }
    if b(p, "no_notify") {
        r.notify = false;
    }
    if b(p, "notify") {
        r.notify = true;
    }
    Ok(())
}

const NOT_MAESTRO: &str = "Error: this terminal is not the Maestro — the user must turn on Maestro mode on your terminal (the crown in its header) to unlock this command";

impl RoutinesState {
    pub async fn run_cli(&self, app: &AppHandle, pty: &Arc<Mutex<PtyStateInner>>, from: &str, p: &Value) -> String {
        if !pty.lock().unwrap().maestros.contains(from) {
            return NOT_MAESTRO.into();
        }
        if self.0.lock().unwrap().scene.is_none() {
            return "Error: no scene loaded".into();
        }
        let verb = s(p, "verb").unwrap_or("");
        let now = Local::now();
        refresh_labels(self, &pty.lock().unwrap().labels);
        let result: Result<String, String> = match verb {
            "list" => {
                let inner = self.0.lock().unwrap();
                Ok(if inner.routines.is_empty() {
                    "(no routines in this scene — `narrater routine create` adds one)".into()
                } else {
                    inner.routines.iter().map(line).collect::<Vec<_>>().join("\n")
                })
            }
            "show" => {
                let mut inner = self.0.lock().unwrap();
                select(&mut inner.routines, p).map(|r| detail(r))
            }
            "create" => {
                let target = {
                    let inner = pty.lock().unwrap();
                    target_from(p, &inner, from).map(|t| {
                        t.unwrap_or_else(|| RoutineTarget::Terminal { id: from.to_string(), label: inner.label_of(from) })
                    })
                };
                (|| {
                    let target = target?;
                    let name = s(p, "name").ok_or("a routine needs a name")?;
                    let command = p.get("command").and_then(Value::as_str).filter(|c| !c.trim().is_empty())
                        .ok_or("a routine needs --command")?;
                    let schedule = schedule_from(p, now)?.ok_or("a routine needs one schedule: --every, --daily, --weekly or --once")?;
                    let mut r = Routine {
                        id: uuid::Uuid::new_v4().simple().to_string(),
                        name: name.to_string(),
                        command: command.to_string(),
                        schedule,
                        target,
                        count: None,
                        until: None,
                        pre_run: None,
                        skip_if_busy: true,
                        notify: true,
                        enabled: !b(p, "disabled"),
                        finished: false,
                        fired: 0,
                        skipped: 0,
                        last_fired: None,
                        next_fire: None,
                        last_result: None,
                    };
                    apply_options(&mut r, p)?;
                    if r.pre_run.is_none() && r.command.contains("{{output}}") {
                        return Err("{{output}} needs a --pre-run script to fill it".into());
                    }
                    arm(&mut r, now);
                    let msg = format!("ok: created\n{}", line(&r));
                    self.0.lock().unwrap().routines.push(r);
                    Ok(msg)
                })()
            }
            "edit" => {
                let target = {
                    let inner = pty.lock().unwrap();
                    target_from(p, &inner, from)
                };
                let schedule = schedule_from(p, now);
                let mut inner = self.0.lock().unwrap();
                (|| {
                    let (target, schedule) = (target?, schedule?);
                    let r = select(&mut inner.routines, p)?;
                    if let Some(n) = s(p, "new_name") {
                        r.name = n.to_string();
                    }
                    if let Some(c) = p.get("command").and_then(Value::as_str).filter(|c| !c.trim().is_empty()) {
                        r.command = c.to_string();
                    }
                    if let Some(t) = target {
                        r.target = t;
                    }
                    if let Some(sch) = schedule {
                        // A new schedule starts the counters over
                        r.schedule = sch;
                        r.fired = 0;
                        r.skipped = 0;
                        r.finished = false;
                        r.enabled = true;
                    }
                    apply_options(r, p)?;
                    arm(r, now);
                    Ok(format!("ok: updated\n{}", line(r)))
                })()
            }
            "enable" | "disable" => {
                let mut inner = self.0.lock().unwrap();
                select(&mut inner.routines, p).map(|r| {
                    r.enabled = verb == "enable";
                    r.finished = false;
                    arm(r, now);
                    format!("ok: {}", line(r))
                })
            }
            "delete" => {
                let mut inner = self.0.lock().unwrap();
                select(&mut inner.routines, p).map(|r| (r.id.clone(), r.name.clone())).map(|(id, name)| {
                    inner.routines.retain(|r| r.id != id);
                    format!("ok: routine '{}' deleted", name)
                })
            }
            "run" => {
                let id = {
                    let mut inner = self.0.lock().unwrap();
                    select(&mut inner.routines, p).map(|r| r.id.clone())
                };
                match id {
                    Ok(id) => return fire(self.clone(), Arc::clone(pty), app.clone(), id, true).await,
                    Err(e) => Err(e),
                }
            }
            _ => Err("unknown routine command — run `narrater help routine`".into()),
        };
        match result {
            Ok(text) => {
                if verb != "list" && verb != "show" {
                    self.changed(app);
                }
                text
            }
            Err(e) => format!("Error: {}", e),
        }
    }
}

// ---- Tauri commands (canvas / sidebar) -----------------------------------

/// Loads the routines of the scene that just opened (the scheduler only
/// fires the current scene's routines — its terminals are the live ones).
#[tauri::command]
pub fn routines_load(scene: String, app: AppHandle, state: State<'_, RoutinesState>) -> Vec<RoutineView> {
    let mut routines: Vec<Routine> = std::fs::read_to_string(routines_path(&scene))
        .ok()
        .and_then(|raw| serde_json::from_str::<Value>(&raw).ok())
        .and_then(|v| serde_json::from_value(v["routines"].clone()).ok())
        .unwrap_or_default();
    let now = Local::now();
    for r in routines.iter_mut() {
        arm(r, now);
    }
    {
        let mut inner = state.0.lock().unwrap();
        inner.scene = Some(scene);
        inner.routines = routines;
    }
    state.changed(&app);
    state.views()
}

#[tauri::command]
pub fn routines_list(state: State<'_, RoutinesState>) -> Vec<RoutineView> {
    state.views()
}

/// The New Terminal dialog's "scheduled prompt": every N seconds into it.
#[tauri::command]
pub fn routine_ui_create(
    name: String,
    command: String,
    every_secs: u64,
    terminal_id: String,
    terminal_label: String,
    app: AppHandle,
    state: State<'_, RoutinesState>,
) -> Result<(), String> {
    if every_secs < schedule::MIN_EVERY_SECS {
        return Err(format!("the interval must be at least {}s", schedule::MIN_EVERY_SECS));
    }
    let mut r = Routine {
        id: uuid::Uuid::new_v4().simple().to_string(),
        name,
        command,
        schedule: Schedule::Every { secs: every_secs },
        target: RoutineTarget::Terminal { id: terminal_id, label: terminal_label },
        count: None,
        until: None,
        pre_run: None,
        skip_if_busy: true,
        notify: false,
        enabled: true,
        finished: false,
        fired: 0,
        skipped: 0,
        last_fired: None,
        next_fire: None,
        last_result: None,
    };
    arm(&mut r, Local::now());
    state.0.lock().unwrap().routines.push(r);
    state.changed(&app);
    Ok(())
}

#[tauri::command]
pub fn routine_set_enabled(id: String, enabled: bool, app: AppHandle, state: State<'_, RoutinesState>) -> Result<(), String> {
    {
        let mut inner = state.0.lock().unwrap();
        let r = inner.routines.iter_mut().find(|r| r.id == id).ok_or("routine not found")?;
        r.enabled = enabled;
        r.finished = false;
        arm(r, Local::now());
    }
    state.changed(&app);
    Ok(())
}

#[tauri::command]
pub fn routine_delete(id: String, app: AppHandle, state: State<'_, RoutinesState>) {
    state.0.lock().unwrap().routines.retain(|r| r.id != id);
    state.changed(&app);
}

#[tauri::command]
pub async fn routine_run(
    id: String,
    app: AppHandle,
    state: State<'_, RoutinesState>,
    pty: State<'_, PtyState>,
) -> Result<String, String> {
    Ok(fire(state.inner().clone(), Arc::clone(&pty.0), app, id, true).await)
}

/// Routines follow their scene's renames and deletions.
pub fn rename_scene(old: &str, new: &str, state: &RoutinesState) {
    let (from, to) = (routines_path(old), routines_path(new));
    if from.exists() {
        let _ = std::fs::rename(from, to);
    }
    let mut inner = state.0.lock().unwrap();
    if inner.scene.as_deref() == Some(old) {
        inner.scene = Some(new.to_string());
    }
}

pub fn delete_scene(name: &str, state: &RoutinesState) {
    let _ = std::fs::remove_file(routines_path(name));
    let mut inner = state.0.lock().unwrap();
    if inner.scene.as_deref() == Some(name) {
        inner.routines.clear();
    }
}

/// Terminal labels for display, refreshed from the live sessions.
pub fn refresh_labels(state: &RoutinesState, labels: &HashMap<String, String>) {
    let mut inner = state.0.lock().unwrap();
    for r in inner.routines.iter_mut() {
        if let RoutineTarget::Terminal { id, label } = &mut r.target {
            if let Some(l) = labels.get(id) {
                label.clone_from(l);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn routine(name: &str, id: &str) -> Routine {
        Routine {
            id: id.into(),
            name: name.into(),
            command: "c".into(),
            schedule: Schedule::Every { secs: 60 },
            target: RoutineTarget::Reminder,
            count: None,
            until: None,
            pre_run: None,
            skip_if_busy: true,
            notify: true,
            enabled: true,
            finished: false,
            fired: 0,
            skipped: 0,
            last_fired: None,
            next_fire: None,
            last_result: None,
        }
    }

    #[test]
    fn select_by_name_and_id_prefix() {
        let mut rs = vec![routine("Audit", "a1b2c3d4"), routine("Audit", "ff0011aa"), routine("Digest", "123456")];
        assert_eq!(select(&mut rs, &json!({ "name": "digest" })).unwrap().id, "123456");
        let err = select(&mut rs, &json!({ "name": "Audit" })).unwrap_err();
        assert!(err.contains("a1b2c3") && err.contains("ff0011") && err.contains("--id"));
        assert_eq!(select(&mut rs, &json!({ "name": "Audit", "id": "ff" })).unwrap().id, "ff0011aa");
        assert!(select(&mut rs, &json!({ "name": "Nope" })).is_err());
    }

    #[test]
    fn exactly_one_schedule() {
        let now = Local::now();
        assert!(schedule_from(&json!({ "every": "30m", "daily": "09:00" }), now).is_err());
        assert_eq!(schedule_from(&json!({}), now), Ok(None));
        assert_eq!(schedule_from(&json!({ "every": "2h" }), now), Ok(Some(Schedule::Every { secs: 7200 })));
    }

    #[test]
    fn arming_finishes_spent_routines() {
        let now = Local::now();
        let mut once = routine("Once", "x");
        once.schedule = Schedule::Once { at: "2000-01-01 00:00".into() };
        // Missed while closed: fires right away
        arm(&mut once, now);
        assert_eq!(once.next_fire, Some(now.timestamp()));
        once.fired = 1;
        arm(&mut once, now);
        assert!(once.finished && !once.enabled);

        let mut old = routine("Old", "y");
        old.until = Some("2000-01-01".into());
        arm(&mut old, now);
        assert!(old.finished);
    }

    #[test]
    fn options_validate() {
        let mut r = routine("R", "z");
        assert!(apply_options(&mut r, &json!({ "count": 0 })).is_err());
        apply_options(&mut r, &json!({ "count": 3, "until": "2030-01-01", "no_skip_if_busy": true, "no_notify": true, "pre_run": "date" })).unwrap();
        assert_eq!((r.count, r.until.as_deref(), r.skip_if_busy, r.notify, r.pre_run.as_deref()), (Some(3), Some("2030-01-01"), false, false, Some("date")));
    }

    #[tokio::test]
    async fn pre_run_output_and_failure() {
        let dir = std::env::temp_dir();
        let env = [("NARRATER_ROUTINE_NAME", "R".to_string())];
        assert_eq!(pre_run("echo \"$NARRATER_ROUTINE_NAME ok\"", dir.clone(), &env).await, Ok("R ok".into()));
        let err = pre_run("echo boom >&2; exit 3", dir, &env).await.unwrap_err();
        assert!(err.contains("exited with 3") && err.contains("boom"));
    }
}
