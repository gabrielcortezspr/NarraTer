//! Manual probe for the screen-state heuristics against a real program.
//!
//!   cargo run --example turn_probe -- [--shell] "<prompt>" <command> [args…]
//!
//! Spawns the command in a PTY, waits until it's idle, types the prompt with
//! the same delivery plan the app uses, then follows the turn state until it
//! settles and prints the final screen.

use std::io::{Read, Write};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use narrater_lib::screen::{classify, plan_delivery, ScreenModel, TurnState};
use portable_pty::{native_pty_system, CommandBuilder, PtySize};

fn main() {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let is_shell = args.first().is_some_and(|a| a == "--shell");
    if is_shell {
        args.remove(0);
    }
    if args.len() < 2 {
        eprintln!("usage: turn_probe [--shell] \"<prompt>\" <command> [args…]");
        std::process::exit(2);
    }
    let prompt = args.remove(0);
    let (rows, cols) = (40u16, 120u16);

    let pair = native_pty_system()
        .openpty(PtySize { rows, cols, pixel_width: 0, pixel_height: 0 })
        .expect("openpty");
    let mut cmd = CommandBuilder::new(&args[0]);
    for a in &args[1..] {
        cmd.arg(a);
    }
    cmd.cwd(std::env::current_dir().unwrap());
    cmd.env("TERM", "xterm-256color");
    let mut child = pair.slave.spawn_command(cmd).expect("spawn");
    let pid = child.process_id().map(|p| p as i32);
    drop(pair.slave);
    let mut writer = pair.master.take_writer().unwrap();
    let mut reader = pair.master.try_clone_reader().unwrap();

    let screen = Arc::new(Mutex::new(ScreenModel::new(rows, cols)));
    let s2 = Arc::clone(&screen);
    std::thread::spawn(move || {
        let mut buf = [0u8; 8192];
        while let Ok(n) = reader.read(&mut buf) {
            if n == 0 {
                break;
            }
            s2.lock().unwrap().feed(&buf[..n]);
        }
    });

    let started = Instant::now();
    let state = |now: Instant| -> (TurnState, bool) {
        let mut s = screen.lock().unwrap();
        s.refresh(now);
        let fg = if is_shell {
            pair.master.process_group_leader().zip(pid).map(|(l, p)| l != p)
        } else {
            None
        };
        let since = now.saturating_duration_since(s.last_change);
        (classify(&s.footer(), since, is_shell, fg), s.has_output)
    };
    let mut last = None;
    let mut wait_for = |label: &str, max: Duration, done: &dyn Fn(TurnState) -> bool| -> TurnState {
        let t0 = Instant::now();
        loop {
            std::thread::sleep(Duration::from_millis(200));
            let (turn, has_output) = state(Instant::now());
            if last != Some(turn) {
                println!("[{:>6.1}s] {:<8} {}", started.elapsed().as_secs_f32(), label, turn.as_str());
                last = Some(turn);
            }
            if has_output && done(turn) {
                return turn;
            }
            if t0.elapsed() > max {
                println!("[{:>6.1}s] {} timed out", started.elapsed().as_secs_f32(), label);
                return turn;
            }
        }
    };

    wait_for("boot", Duration::from_secs(60), &|t| t == TurnState::Idle);
    let bracketed = screen.lock().unwrap().bracketed_paste();
    println!("bracketed paste enabled by the program: {}", bracketed);

    let delivered = Instant::now();
    for step in plan_delivery(&prompt, bracketed, is_shell) {
        std::thread::sleep(Duration::from_millis(step.delay_ms));
        writer.write_all(step.data.as_bytes()).unwrap();
        writer.flush().unwrap();
    }
    screen.lock().unwrap().last_change = Instant::now();
    let end = wait_for("turn", Duration::from_secs(240), &|t| {
        delivered.elapsed() > Duration::from_millis(800) && t != TurnState::Working
    });
    println!("turn ended as '{}' after {:.1}s\n----- screen -----", end.as_str(), delivered.elapsed().as_secs_f32());
    println!("{}", screen.lock().unwrap().text());
    let _ = child.kill();
}
