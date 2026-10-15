//! Measures the turn-state reader against ground truth.
//!
//!   cargo run --release --example turn_eval -- [--reps N] [--claude] [--tmux-fg none|pane] [--out DIR] [--only NAME]
//!
//! Each scenario runs a real program in a PTY, types a prompt with the app's
//! delivery plan and follows the turn state with the app's own `ScreenModel`
//! and `classify`, using the timings of `narrater ask` (an ask returns on the
//! first non-working state at least MIN_TURN after delivery). The *oracle* is
//! an independent signal of when the turn really ended:
//! - shells: the command itself writes a timestamp when it finishes;
//! - Claude Code: its `Stop` / `Notification` hooks write timestamps. The
//!   hooks exist only here, to grade the screen reader; the app never needs
//!   them (D01).
//!
//! Per run it reports the outcome, the latency (detection − oracle; negative
//! means the ask returned before the turn ended) and writes a timeline of
//! states, titles and footers to `<out>/<scenario>-<rep>.jsonl` plus the raw
//! output as an asciicast (`.cast`) for offline replay. A summary table goes
//! to stdout and `<out>/summary.json`.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use narrater_lib::screen::{classify, foreground_busy, plan_delivery, tmux_pane_pid, ScreenModel, TurnState};
use portable_pty::{native_pty_system, CommandBuilder, PtySize};

/// Same as `ipc.rs`: an ask reads the answer only this long after delivery.
const MIN_TURN: Duration = Duration::from_millis(800);
/// The app's status monitor tick (`pty.rs`).
const TICK: Duration = Duration::from_millis(200);
const ROWS: u16 = 40;
const COLS: u16 = 120;
const TMUX_SOCKET: &str = "narrater-eval";

#[derive(Clone)]
enum Kind {
    /// A shell command; `tmux`: the shell runs inside a tmux session, as the
    /// app's persistent terminals do.
    Shell { cmd: &'static str, tmux: bool },
    /// A Claude Code prompt; `expect_attention`: the turn should stop at a
    /// permission prompt (oracle: the Notification hook).
    Claude { prompt: &'static str, args: &'static [&'static str], expect_attention: bool },
}

#[derive(Clone)]
struct Scenario {
    name: &'static str,
    kind: Kind,
}

fn scenarios() -> Vec<Scenario> {
    let shell = |name, cmd, tmux| Scenario { name, kind: Kind::Shell { cmd, tmux } };
    let claude = |name, prompt, args, expect_attention| Scenario { name, kind: Kind::Claude { prompt, args, expect_attention } };
    vec![
        shell("shell-silent", "sleep 4", false),
        shell("shell-ticking", "for i in $(seq 1 12); do echo tick $i; sleep 0.3; done", false),
        shell("shell-burst-then-silent", "seq 1 300; sleep 3", false),
        shell("tmux-silent", "sleep 4", true),
        shell("tmux-ticking", "for i in $(seq 1 12); do echo tick $i; sleep 0.3; done", true),
        shell("tmux-burst-then-silent", "seq 1 300; sleep 3", true),
        claude("claude-short", "Reply with exactly PONG and nothing else.", &[], false),
        claude(
            "claude-silent-tool",
            "Use the Bash tool to run `sleep 8`, then reply with exactly DONE.",
            &["--allowedTools", "Bash"],
            false,
        ),
        claude("claude-long-answer", "Write about 250 words on the history of terminal emulators. No tools.", &[], false),
        claude(
            "claude-permission",
            "Use the Bash tool to run `touch narrater-eval-marker.txt`.",
            &[],
            true,
        ),
    ]
}

fn epoch_ms() -> f64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs_f64() * 1000.0
}

/// First timestamp (epoch ns, one per line) in `path` at or after `after_ms`.
fn oracle_after(path: &Path, after_ms: f64) -> Option<f64> {
    let text = std::fs::read_to_string(path).ok()?;
    text.lines()
        .filter_map(|l| l.split_whitespace().next()?.parse::<f64>().ok())
        .map(|ns| ns / 1e6)
        .find(|&ms| ms >= after_ms)
}

fn json_str(s: &str) -> String {
    serde_json::to_string(s).unwrap()
}

struct RunResult {
    scenario: String,
    rep: usize,
    outcome: String,
    expected: String,
    latency_ms: Option<f64>,
    note: String,
}

#[allow(clippy::too_many_arguments)]
fn run(sc: &Scenario, rep: usize, out: &Path, work: &Path, tmux_fg: &str, tmux: &str) -> RunResult {
    let session = format!("eval-{}-{}", sc.name, rep);
    let oracle = work.join(format!("{}-{}.oracle", sc.name, rep));
    let notif = work.join(format!("{}-{}.notif", sc.name, rep));
    let perm = work.join(format!("{}-{}.perm", sc.name, rep));
    for f in [&oracle, &notif, &perm] {
        let _ = std::fs::remove_file(f);
    }

    let pair = native_pty_system()
        .openpty(PtySize { rows: ROWS, cols: COLS, pixel_width: 0, pixel_height: 0 })
        .expect("openpty");
    let (mut cmd, is_shell, in_tmux) = match &sc.kind {
        Kind::Shell { tmux: true, .. } => {
            let mut c = CommandBuilder::new(tmux);
            for a in ["-L", TMUX_SOCKET, "-f", "/dev/null", "new-session", "-A", "-s", &session, "bash", "--norc", "--noprofile"] {
                c.arg(a);
            }
            (c, true, true)
        }
        Kind::Shell { .. } => {
            let mut c = CommandBuilder::new("bash");
            c.args(["--norc", "--noprofile"]);
            (c, true, false)
        }
        Kind::Claude { args, .. } => {
            let settings = work.join(format!("{}-{}.settings.json", sc.name, rep));
            let hook = |file: &Path| format!("date +%s%N >> {}", file.display());
            let s = serde_json::json!({
                "hooks": {
                    "Stop": [{ "hooks": [{ "type": "command", "command": hook(&oracle) }] }],
                    "Notification": [{ "hooks": [{ "type": "command", "command": hook(&notif) }] }],
                    // Fires when the permission dialog is shown (Notification can come later)
                    "PermissionRequest": [{ "hooks": [{ "type": "command", "command": hook(&perm) }] }],
                }
            });
            std::fs::write(&settings, s.to_string()).unwrap();
            let mut c = CommandBuilder::new("claude");
            c.args(["--model", "haiku", "--settings"]);
            c.arg(settings.display().to_string());
            for a in args.iter() {
                c.arg(a);
            }
            (c, false, false)
        }
    };
    cmd.cwd(work);
    cmd.env("TERM", "xterm-256color");
    cmd.env("PS1", "$ ");
    for k in ["CLAUDECODE", "CLAUDE_CODE_ENTRYPOINT", "CLAUDE_CODE_SESSION_ID"] {
        cmd.env_remove(k);
    }
    let mut child = pair.slave.spawn_command(cmd).expect("spawn");
    let pid = child.process_id().map(|p| p as i32);
    drop(pair.slave);
    let mut writer = pair.master.take_writer().unwrap();
    let mut reader = pair.master.try_clone_reader().unwrap();

    let started = Instant::now();
    let cast = Arc::new(Mutex::new(std::fs::File::create(out.join(format!("{}-{}.cast", sc.name, rep))).unwrap()));
    writeln!(cast.lock().unwrap(), "{{\"version\": 2, \"width\": {}, \"height\": {}}}", COLS, ROWS).unwrap();
    let screen = Arc::new(Mutex::new(ScreenModel::new(ROWS, COLS)));
    let (s2, c2) = (Arc::clone(&screen), Arc::clone(&cast));
    std::thread::spawn(move || {
        let mut buf = [0u8; 8192];
        while let Ok(n) = reader.read(&mut buf) {
            if n == 0 {
                break;
            }
            s2.lock().unwrap().feed(&buf[..n]);
            let data = String::from_utf8_lossy(&buf[..n]);
            let _ = writeln!(c2.lock().unwrap(), "[{:.4}, \"o\", {}]", started.elapsed().as_secs_f64(), json_str(&data));
        }
    });

    let mut pane_pid: Option<i32> = None;
    let mut timeline = std::fs::File::create(out.join(format!("{}-{}.jsonl", sc.name, rep))).unwrap();
    let mut state = |now: Instant, phase: &str, timeline: &mut std::fs::File| -> TurnState {
        let fg = if !is_shell {
            None
        } else if in_tmux {
            if tmux_fg == "pane" {
                if pane_pid.is_none() {
                    pane_pid = tmux_pane_pid(tmux, TMUX_SOCKET, &session);
                }
                pane_pid.and_then(foreground_busy)
            } else {
                None // the app today: inside tmux only the screen tells
            }
        } else {
            pair.master.process_group_leader().zip(pid).map(|(l, p)| l != p)
        };
        let mut s = screen.lock().unwrap();
        s.refresh(now);
        if fg == Some(true) {
            s.mark_active(now); // as compute_turn in pty.rs
        }
        let since = now.saturating_duration_since(s.last_change);
        let footer = s.footer();
        let turn = classify(&footer, s.title(), since, is_shell, fg);
        let last_line = footer.lines().last().unwrap_or("").to_string();
        let _ = writeln!(
            timeline,
            "{{\"t\": {:.3}, \"phase\": {}, \"state\": \"{}\", \"fg\": {}, \"title\": {}, \"footer\": {}}}",
            started.elapsed().as_secs_f64(),
            json_str(phase),
            turn.as_str(),
            fg.map_or("null".into(), |b| b.to_string()),
            json_str(s.title()),
            json_str(&last_line)
        );
        turn
    };

    // Boot: idle with output for a full second (the app gates delivery on idle too)
    let boot_deadline = Instant::now() + Duration::from_secs(60);
    let mut idle_since: Option<Instant> = None;
    loop {
        std::thread::sleep(TICK);
        let now = Instant::now();
        let has_output = screen.lock().unwrap().has_output;
        if has_output && state(now, "boot", &mut timeline) == TurnState::Idle {
            if idle_since.get_or_insert(now).elapsed() >= Duration::from_secs(1) {
                break;
            }
        } else {
            idle_since = None;
        }
        if now > boot_deadline {
            let _ = child.kill();
            return RunResult {
                scenario: sc.name.into(),
                rep,
                outcome: "boot-timeout".into(),
                expected: "-".into(),
                latency_ms: None,
                note: "never idle at boot".into(),
            };
        }
    }

    let (prompt, expected) = match &sc.kind {
        Kind::Shell { cmd, .. } => (format!("{}; date +%s%N >> {}", cmd, oracle.display()), "idle"),
        Kind::Claude { prompt, expect_attention, .. } => {
            (prompt.to_string(), if *expect_attention { "attention" } else { "idle" })
        }
    };
    let bracketed = screen.lock().unwrap().bracketed_paste();
    let delivered_epoch = epoch_ms();
    let delivered = Instant::now();
    for step in plan_delivery(&prompt, bracketed, is_shell) {
        std::thread::sleep(Duration::from_millis(step.delay_ms));
        writer.write_all(step.data.as_bytes()).unwrap();
        writer.flush().unwrap();
    }
    // Typing the prompt changes the screen; the app counts delivery as activity
    screen.lock().unwrap().last_change = Instant::now();

    // Follow the turn the way `narrater ask` does
    let mut detected: Option<(TurnState, f64)> = None;
    let deadline = delivered + Duration::from_secs(180);
    loop {
        std::thread::sleep(TICK);
        let now = Instant::now();
        let turn = state(now, "turn", &mut timeline);
        if detected.is_none() && delivered.elapsed() >= MIN_TURN && turn != TurnState::Working {
            detected = Some((turn, epoch_ms()));
        }
        let truth = oracle_after(&oracle, delivered_epoch)
            .or_else(|| oracle_after(&perm, delivered_epoch))
            .or_else(|| oracle_after(&notif, delivered_epoch));
        // Keep observing a little past both signals, for the timeline
        // A permission prompt is kept open longer: what it does while it waits
        // (Claude blinks a bullet) is what the reader must cope with
        let linger = if expected == "attention" { 8000.0 } else { 1500.0 };
        if let (Some(_), Some(t)) = (detected, truth) {
            if epoch_ms() - t.max(detected.unwrap().1) > linger {
                break;
            }
        }
        if now > deadline {
            break;
        }
    }
    if expected == "attention" {
        // Leave the permission prompt (Esc) so the agent can exit cleanly
        let _ = writer.write_all(b"\x1b");
    }
    let _ = child.kill();
    if in_tmux {
        let _ = std::process::Command::new(tmux).args(["-L", TMUX_SOCKET, "kill-session", "-t", &session]).output();
    }

    // Attention: the earliest of PermissionRequest and Notification. Both are
    // reported, to keep the choice of oracle visible
    let (perm_t, notif_t) = (oracle_after(&perm, delivered_epoch), oracle_after(&notif, delivered_epoch));
    let truth = match expected {
        "attention" => match (perm_t, notif_t) {
            (Some(p), Some(n)) => Some(p.min(n)),
            (p, n) => p.or(n),
        },
        _ => oracle_after(&oracle, delivered_epoch),
    };
    if expected == "attention" {
        println!(
            "    oráculos de permissão (ms após a entrega): PermissionRequest={:?} Notification={:?}",
            perm_t.map(|t| (t - delivered_epoch).round()),
            notif_t.map(|t| (t - delivered_epoch).round())
        );
    }
    // Sidecar for offline replay (`tests/turn_transcripts.rs`): delivery and
    // oracle on the recording's clock
    let delivered_s = delivered.duration_since(started).as_secs_f64();
    let meta = serde_json::json!({
        "scenario": sc.name,
        "is_shell": is_shell,
        "expected": expected,
        "delivered_s": delivered_s,
        "oracle_s": truth.map(|t| delivered_s + (t - delivered_epoch) / 1000.0),
    });
    let _ = std::fs::write(out.join(format!("{}-{}.meta.json", sc.name, rep)), serde_json::to_string_pretty(&meta).unwrap());
    let outcome = detected.map_or("none".to_string(), |(t, _)| t.as_str().replace("running", "working"));
    let latency_ms = detected.zip(truth).map(|((_, d), t)| d - t);
    let note = match (detected, truth) {
        (None, _) => "never detected".into(),
        (_, None) => "no oracle signal".into(),
        (Some(_), Some(_)) if latency_ms.unwrap() < 0.0 => "PREMATURE: returned before the turn ended".into(),
        _ => String::new(),
    };
    RunResult { scenario: sc.name.into(), rep, outcome, expected: expected.into(), latency_ms, note }
}

fn main() {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let mut take = |flag: &str| -> Option<String> {
        let i = args.iter().position(|a| a == flag)?;
        let v = args.get(i + 1).cloned();
        args.drain(i..(i + 2).min(args.len()));
        v
    };
    let reps: usize = take("--reps").and_then(|v| v.parse().ok()).unwrap_or(3);
    let tmux_fg = take("--tmux-fg").unwrap_or_else(|| "none".into());
    let only = take("--only");
    let out = PathBuf::from(take("--out").unwrap_or_else(|| "target/turn-eval".into()));
    let with_claude = args.iter().any(|a| a == "--claude");
    let tmux = which("tmux").unwrap_or_else(|| "tmux".into());

    let _ = std::fs::remove_dir_all(&out);
    std::fs::create_dir_all(&out).unwrap();
    let work = std::fs::canonicalize(&out).unwrap().join("work");
    std::fs::create_dir_all(&work).unwrap();
    if with_claude {
        pre_trust(&work);
    }

    let mut results = Vec::new();
    for sc in scenarios() {
        if matches!(sc.kind, Kind::Claude { .. }) && !with_claude {
            continue;
        }
        if only.as_deref().is_some_and(|o| !sc.name.contains(o)) {
            continue;
        }
        for rep in 1..=reps {
            let r = run(&sc, rep, &out, &work, &tmux_fg, &tmux);
            println!(
                "{:<26} #{}  detected={:<9} expected={:<9} latency={:>9}  {}",
                r.scenario,
                r.rep,
                r.outcome,
                r.expected,
                r.latency_ms.map_or("-".into(), |l| format!("{:.0} ms", l)),
                r.note
            );
            results.push(r);
        }
    }

    // Summary per scenario
    println!("\n{:<26} {:>4} {:>8} {:>10} {:>12} {:>12}", "scenario", "runs", "correct", "premature", "median lat.", "max lat.");
    let mut summary = Vec::new();
    let mut names: Vec<String> = results.iter().map(|r| r.scenario.clone()).collect();
    names.dedup();
    for name in names {
        let rs: Vec<&RunResult> = results.iter().filter(|r| r.scenario == name).collect();
        let correct = rs.iter().filter(|r| r.outcome == r.expected && r.latency_ms.is_some_and(|l| l >= 0.0)).count();
        let premature = rs.iter().filter(|r| r.latency_ms.is_some_and(|l| l < 0.0)).count();
        let mut lats: Vec<f64> = rs.iter().filter_map(|r| r.latency_ms).collect();
        lats.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let median = lats.get(lats.len() / 2).copied();
        let max = lats.last().copied();
        let f = |v: Option<f64>| v.map_or("-".to_string(), |v| format!("{:.0} ms", v));
        println!("{:<26} {:>4} {:>8} {:>10} {:>12} {:>12}", name, rs.len(), correct, premature, f(median), f(max));
        summary.push(serde_json::json!({
            "scenario": name, "runs": rs.len(), "correct": correct, "premature": premature,
            "latencies_ms": lats, "outcomes": rs.iter().map(|r| r.outcome.clone()).collect::<Vec<_>>(),
        }));
    }
    std::fs::write(out.join("summary.json"), serde_json::to_string_pretty(&summary).unwrap()).unwrap();
    let _ = std::process::Command::new(&tmux).args(["-L", TMUX_SOCKET, "kill-server"]).output();
}

fn which(name: &str) -> Option<String> {
    std::env::var("PATH").ok()?.split(':').map(|d| Path::new(d).join(name)).find(|p| p.exists()).map(|p| p.display().to_string())
}

/// Claude Code asks whether to trust a new folder; the app pre-trusts the
/// terminal's directory the same way (`pre_trust_claude_dir` in pty.rs).
fn pre_trust(dir: &Path) {
    let Some(home) = std::env::var_os("HOME").map(PathBuf::from) else { return };
    let path = home.join(".claude.json");
    let mut root: serde_json::Value =
        std::fs::read_to_string(&path).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or(serde_json::json!({}));
    root["projects"][dir.display().to_string()]["hasTrustDialogAccepted"] = serde_json::json!(true);
    // A user's machine has been through the first-run screens (theme, login notice)
    root["hasCompletedOnboarding"] = serde_json::json!(true);
    if root.get("theme").is_none() {
        root["theme"] = serde_json::json!("dark");
    }
    std::fs::write(&path, serde_json::to_string_pretty(&root).unwrap()).unwrap();
}
