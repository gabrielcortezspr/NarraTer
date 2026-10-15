//! Regression test of the turn-state reader against real transcripts.
//!
//! Each `tests/transcripts/<name>.cast` is a recording made by
//! `examples/turn_eval.rs` (a real Claude Code or shell session), with a
//! `<name>.meta.json` sidecar holding when the prompt was delivered and when
//! the turn really ended (the oracle: a Claude Code hook or the command's own
//! timestamp). Replaying the bytes through `ScreenModel` + `classify` on the
//! app's 200 ms tick, an `ask` must return:
//! - not before the oracle (a premature answer is a half-written screen);
//! - within MAX_LATENCY after it;
//! - with the expected state (idle, or attention for a permission prompt).
//!
//! Shell recordings are replayed without the foreground-process signal (it
//! can't be recorded), so they only cover the screen side.

use std::path::Path;
use std::time::{Duration, Instant};

use narrater_lib::screen::{classify, ScreenModel, TurnState};

const TICK: f64 = 0.2;
const MIN_TURN: f64 = 0.8;
/// The title-aware reader measured ≤ 0.67 s on Claude Code; the screen-only
/// one 1.3–2.1 s (docs/pesquisa/leitor-de-estado.md).
const MAX_LATENCY: f64 = 1.0;

fn replay(cast: &Path) -> (Vec<(f64, String)>, u16, u16) {
    let text = std::fs::read_to_string(cast).unwrap();
    let mut lines = text.lines();
    let header: serde_json::Value = serde_json::from_str(lines.next().unwrap()).unwrap();
    let events = lines
        .filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok())
        .filter_map(|e| Some((e[0].as_f64()?, e[2].as_str()?.to_string())))
        .collect();
    (events, header["height"].as_u64().unwrap() as u16, header["width"].as_u64().unwrap() as u16)
}

/// When an ask returns on this recording, and with which state.
fn ask_returns(cast: &Path, delivered: f64, is_shell: bool) -> Option<(f64, TurnState)> {
    let (events, rows, cols) = replay(cast);
    let base = Instant::now();
    let mut m = ScreenModel::new(rows, cols);
    let (mut next, mut t) = (0, 0.0);
    let end = events.last()?.0 + 5.0;
    let mut bumped = false;
    while t <= end {
        while next < events.len() && events[next].0 <= t {
            m.feed(events[next].1.as_bytes());
            next += 1;
        }
        let now = base + Duration::from_secs_f64(t);
        if !bumped && t >= delivered {
            m.mark_active(now); // delivery counts as activity, as in the app
            bumped = true;
        }
        m.refresh(now);
        let state = classify(&m.footer(), m.title(), now.saturating_duration_since(m.last_change), is_shell, None);
        if t >= delivered + MIN_TURN && state != TurnState::Working {
            return Some((t, state));
        }
        t += TICK;
    }
    None
}

#[test]
fn the_reader_matches_the_oracle_on_real_transcripts() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/transcripts");
    let mut checked = 0;
    let mut failures = Vec::new();
    for entry in std::fs::read_dir(&dir).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().and_then(|e| e.to_str()) != Some("cast") {
            continue;
        }
        let meta: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(path.with_extension("meta.json")).unwrap()).unwrap();
        let name = path.file_stem().unwrap().to_string_lossy().to_string();
        let (delivered, oracle) = (meta["delivered_s"].as_f64().unwrap(), meta["oracle_s"].as_f64().unwrap());
        let expected = if meta["expected"] == "attention" { TurnState::Attention } else { TurnState::Idle };
        match ask_returns(&path, delivered, meta["is_shell"].as_bool().unwrap()) {
            None => failures.push(format!("{}: never returned", name)),
            Some((t, state)) => {
                let latency = t - oracle;
                if state != expected {
                    failures.push(format!("{}: returned {:?}, expected {:?}", name, state, expected));
                }
                // A tick of slack: the oracle and the recording clock are different processes
                if latency < -TICK {
                    failures.push(format!("{}: premature by {:.2}s", name, -latency));
                }
                if latency > MAX_LATENCY {
                    failures.push(format!("{}: {:.2}s late", name, latency));
                }
            }
        }
        checked += 1;
    }
    assert!(checked > 0, "no transcripts in {}", dir.display());
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
