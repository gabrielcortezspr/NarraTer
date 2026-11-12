//! Replays a recording made by `turn_eval` (asciicast v2) through the app's
//! `ScreenModel` and `classify`, to develop screen rules offline against real
//! transcripts.
//!
//!   cargo run --example turn_replay -- FILE.cast [--at SECONDS] [--agent|--shell]
//!
//! Prints every state transition (time, state, title, last footer line) as the
//! status monitor would see it on its 200 ms tick; with `--at`, also the full
//! screen at that moment.

use std::time::{Duration, Instant};

use narrater_lib::screen::{classify, ScreenModel, TurnState};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(path) = args.first() else {
        eprintln!("usage: turn_replay FILE.cast [--at SECONDS] [--shell]");
        std::process::exit(2);
    };
    let at: Option<f64> = args.iter().position(|a| a == "--at").and_then(|i| args.get(i + 1)?.parse().ok());
    let is_shell = args.iter().any(|a| a == "--shell");

    let text = std::fs::read_to_string(path).expect("read cast");
    let mut lines = text.lines();
    let header: serde_json::Value = serde_json::from_str(lines.next().expect("header")).expect("header json");
    let (cols, rows) = (header["width"].as_u64().unwrap_or(120) as u16, header["height"].as_u64().unwrap_or(40) as u16);
    let events: Vec<(f64, String)> = lines
        .filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok())
        .filter_map(|e| Some((e[0].as_f64()?, e[2].as_str()?.to_string())))
        .collect();
    let end = events.last().map_or(0.0, |e| e.0) + 3.0;

    // Virtual clock: refresh on the app's 200 ms tick
    let base = Instant::now();
    let mut m = ScreenModel::new(rows, cols);
    let mut next = 0;
    let mut last: Option<(TurnState, String, String)> = None;
    let mut printed_at = false;
    let mut t = 0.0;
    while t <= end {
        while next < events.len() && events[next].0 <= t {
            m.feed(events[next].1.as_bytes());
            next += 1;
        }
        let now = base + Duration::from_secs_f64(t);
        m.refresh(now);
        let since = now.saturating_duration_since(m.last_change);
        let footer = m.footer();
        let state = classify(&footer, m.title(), since, is_shell, None);
        let line = footer.lines().last().unwrap_or("").to_string();
        let key = (state, m.title().to_string(), line.clone());
        if last.as_ref() != Some(&key) {
            println!("{:>7.1}s  {:<9} {:<40} {}", t, state.as_str(), format!("{:?}", m.title()), line);
            last = Some(key);
        }
        if at.is_some_and(|a| t >= a) && !printed_at {
            println!("----- screen at {:.1}s -----\n{}\n----- footer -----\n{}\n-----", t, m.text(), footer);
            printed_at = true;
        }
        t += 0.2;
    }
}
