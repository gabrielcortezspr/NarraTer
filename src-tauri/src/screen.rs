//! Headless terminal model + turn-state detection.
//!
//! Every PTY is mirrored into a `vt100` emulator in the backend, so we know
//! what the terminal is actually *showing* — not just the raw byte stream.
//! That gives us, for any TUI from any vendor:
//! - the rendered screen text (`ask` replies, `check`);
//! - whether the program enabled bracketed paste (prompt delivery);
//! - a content hash, so redraws that don't change the screen don't count as
//!   activity, while spinners/timers do.
//!
//! The turn state (working / idle / attention) is derived from the screen
//! alone. No hooks, no vendor cooperation.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::time::{Duration, Instant};

/// Screen must be unchanged this long before a TUI agent counts as idle.
pub const SETTLE: Duration = Duration::from_millis(1200);
/// Shells settle faster — and their busy state comes from the process group.
pub const SHELL_SETTLE: Duration = Duration::from_millis(600);
/// A "busy" footer on a screen that hasn't changed for this long is stale
/// (working TUIs animate a spinner/timer at least every second).
pub const STALE_BUSY: Duration = Duration::from_secs(20);

const SCROLLBACK: usize = 2000;
/// Lines from the bottom of the screen scanned for footer patterns.
const FOOTER_LINES: usize = 8;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TurnState {
    Working,
    Idle,
    /// Waiting for the user: permission menu, y/n prompt, trust dialog.
    Attention,
}

impl TurnState {
    /// Wire name for the `pty_status` event and `narrater list`.
    pub fn as_str(self) -> &'static str {
        match self {
            TurnState::Working => "running",
            TurnState::Idle => "idle",
            TurnState::Attention => "attention",
        }
    }
}

/// Keeps the window title (OSC 0/2) the program sets: several agent CLIs
/// signal their turn state there (Claude Code puts `✳` in front at rest).
#[derive(Default)]
struct TitleCallbacks {
    title: String,
}

impl vt100::Callbacks for TitleCallbacks {
    fn set_window_title(&mut self, _: &mut vt100::Screen, title: &[u8]) {
        self.title = String::from_utf8_lossy(title).into_owned();
    }
}

pub struct ScreenModel {
    parser: vt100::Parser<TitleCallbacks>,
    dirty: bool,
    hash: u64,
    pub last_change: Instant,
    /// False until the program printed anything — never deliver into a
    /// terminal that is still booting into a blank screen.
    pub has_output: bool,
}

impl ScreenModel {
    pub fn new(rows: u16, cols: u16) -> Self {
        let parser = vt100::Parser::new_with_callbacks(rows.max(1), cols.max(1), SCROLLBACK, TitleCallbacks::default());
        let hash = hash_text(&parser.screen().contents());
        ScreenModel { parser, dirty: false, hash, last_change: Instant::now(), has_output: false }
    }

    pub fn feed(&mut self, bytes: &[u8]) {
        self.parser.process(bytes);
        self.dirty = true;
        self.has_output = true;
    }

    pub fn resize(&mut self, rows: u16, cols: u16) {
        self.parser.screen_mut().set_size(rows.max(1), cols.max(1));
        self.dirty = true;
    }

    /// Recomputes the content hash if new bytes arrived; bumps `last_change`
    /// only when what's on screen actually changed. Called on each monitor tick.
    pub fn refresh(&mut self, now: Instant) {
        if !self.dirty {
            return;
        }
        self.dirty = false;
        // A title change is activity too (spinners animate there)
        let h = hash_text(&(self.parser.screen().contents() + "\u{0}" + &self.parser.callbacks().title));
        if h != self.hash {
            self.hash = h;
            self.last_change = now;
        }
    }

    /// Counts as on-screen activity: a shell's foreground job is running, so
    /// the turn only settles SHELL_SETTLE after the job ends (and the prompt
    /// is redrawn), not on a screen that merely hasn't repainted yet.
    pub fn mark_active(&mut self, now: Instant) {
        self.last_change = now;
    }

    /// The window title last set by the program (OSC 0/2), or "".
    pub fn title(&self) -> &str {
        &self.parser.callbacks().title
    }

    pub fn bracketed_paste(&self) -> bool {
        self.parser.screen().bracketed_paste()
    }

    /// Rendered visible screen, trailing blank lines removed.
    pub fn text(&self) -> String {
        trim_screen(&self.parser.screen().contents())
    }

    /// Lowercased last non-empty lines — where TUIs draw spinners and menus.
    pub fn footer(&self) -> String {
        footer_of(&self.parser.screen().contents())
    }
}

fn hash_text(s: &str) -> u64 {
    let mut h = DefaultHasher::new();
    s.hash(&mut h);
    h.finish()
}

pub fn trim_screen(s: &str) -> String {
    let lines: Vec<&str> = s.lines().map(|l| l.trim_end()).collect();
    let end = lines.iter().rposition(|l| !l.is_empty()).map(|i| i + 1).unwrap_or(0);
    lines[..end].join("\n")
}

pub fn footer_of(s: &str) -> String {
    let lines: Vec<&str> = s.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
    let start = lines.len().saturating_sub(FOOTER_LINES);
    lines[start..].join("\n").to_lowercase()
}

/// After the title says the agent is at rest, the screen must stay unchanged
/// this long (the final answer and footer repaint with the title).
pub const TITLE_SETTLE: Duration = Duration::from_millis(400);

/// What an agent's window title (OSC 0/2) says about its turn. The glyph set
/// follows Orca's catalog (`src/shared/agent-title-glyphs.ts`, MIT) and was
/// checked against Claude Code 2.1.299 transcripts (`turn_eval`): `✳` at
/// rest, `◐◑◒◓` while working.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TitleSignal {
    Working,
    Idle,
    Attention,
    /// No known glyph: the title says nothing about the turn.
    Unknown,
}

pub fn title_signal(title: &str) -> TitleSignal {
    let Some(c) = title.trim_start().chars().next() else { return TitleSignal::Unknown };
    match c as u32 {
        // Braille and quarter-circle spinners; Gemini ✦ (working), ⏲ (silent work)
        0x2800..=0x28FF | 0x25D0..=0x25D3 | 0x2726 | 0x23F2 => TitleSignal::Working,
        // Gemini ✋: waiting for a permission
        0x270B => TitleSignal::Attention,
        // Claude Code ✳, Gemini ◇: at rest
        0x2733 | 0x25C7 => TitleSignal::Idle,
        _ => TitleSignal::Unknown,
    }
}

/// Spinner lines of TUI agents while a turn runs (Claude Code, Codex, …).
fn is_busy_footer(footer: &str) -> bool {
    const BUSY: &[&str] = &["esc to interrupt", "ctrl+c to interrupt", "ctrl-c to interrupt"];
    BUSY.iter().any(|p| footer.contains(p))
}

/// Interactive choice waiting for a human: a numbered menu whose first option
/// is an approval (Claude/Codex/Gemini permission dialogs) or a y/n prompt.
/// Plain questions in an answer ("Would you like…?") are deliberately NOT
/// matched — that agent is idle and can take the next prompt.
fn is_attention_footer(footer: &str) -> bool {
    if ["(y/n)", "[y/n]", "(yes/no)"].iter().any(|p| footer.contains(p)) {
        return true;
    }
    footer.lines().any(|line| {
        let l = line.trim_start_matches(|c: char| !c.is_ascii_alphanumeric());
        l.starts_with("1.")
            && ["yes", "allow", "approve", "proceed", "trust"].iter().any(|w| l.contains(w))
    })
}

/// Turn state from what's on screen.
/// - `title`: the window title (agents that report their state there settle
///   faster: TITLE_SETTLE instead of SETTLE);
/// - `since_change`: time since the screen content (or title) last changed
///   (delivering a prompt bumps it, so a just-prompted agent never reads as idle);
/// - `fg_busy`: shells only — a foreground job other than the shell itself.
pub fn classify(footer: &str, title: &str, since_change: Duration, is_shell: bool, fg_busy: Option<bool>) -> TurnState {
    if is_shell {
        if fg_busy == Some(true) || since_change < SHELL_SETTLE {
            return TurnState::Working;
        }
        return TurnState::Idle;
    }
    let busy_footer = is_busy_footer(footer);
    if busy_footer && since_change < STALE_BUSY {
        return TurnState::Working;
    }
    match title_signal(title) {
        // A spinning title animates (each frame is a change): stale like a busy footer
        TitleSignal::Working if since_change < STALE_BUSY => return TurnState::Working,
        // The title itself says it waits for a permission
        TitleSignal::Attention => return TurnState::Attention,
        TitleSignal::Idle if !busy_footer => {
            // A menu under a resting title: two signals agree, no quiet needed —
            // Claude Code blinks a bullet every 0.6 s while it waits, so the
            // screen never stays unchanged for SETTLE (turn_eval, D11)
            if is_attention_footer(footer) {
                return TurnState::Attention;
            }
            return if since_change < TITLE_SETTLE { TurnState::Working } else { TurnState::Idle };
        }
        _ => {}
    }
    if since_change < SETTLE {
        return TurnState::Working;
    }
    if is_attention_footer(footer) {
        return TurnState::Attention;
    }
    TurnState::Idle
}

/// Is a job other than the shell in the foreground of `shell_pid`'s
/// terminal? Read from /proc: the terminal's foreground process group
/// (`tpgid`) differs from the shell's own group while a job runs. Works for
/// a shell inside tmux, where the PTY's own foreground is the tmux client.
/// None when the process is gone or /proc is unreadable.
pub fn foreground_busy(shell_pid: i32) -> Option<bool> {
    let stat = std::fs::read_to_string(format!("/proc/{}/stat", shell_pid)).ok()?;
    // Fields after the parenthesised command name: state ppid pgrp session tty_nr tpgid
    let rest = &stat[stat.rfind(')')? + 1..];
    let f: Vec<&str> = rest.split_whitespace().collect();
    let pgrp: i32 = f.get(2)?.parse().ok()?;
    let tpgid: i32 = f.get(5)?.parse().ok()?;
    Some(tpgid > 0 && tpgid != pgrp)
}

/// The pid of the shell running in a tmux session's (first) pane.
pub fn tmux_pane_pid(tmux: &str, socket: &str, session: &str) -> Option<i32> {
    let out = std::process::Command::new(tmux)
        .args(["-L", socket, "display-message", "-p", "-t", session, "#{pane_pid}"])
        .output()
        .ok()?;
    String::from_utf8_lossy(&out.stdout).trim().parse().ok()
}

/// One step of a prompt delivery: bytes to write after `delay_ms`.
#[derive(Debug, PartialEq)]
pub struct PtyWriteStep {
    pub data: String,
    pub delay_ms: u64,
}

/// How to type `text` into a terminal so it gets submitted exactly once.
/// Bracketed paste is used only when the program *enabled* it (DECSET 2004,
/// observed by the emulator) — never assumed from the agent type.
pub fn plan_delivery(text: &str, bracketed: bool, is_shell: bool) -> Vec<PtyWriteStep> {
    // Control bytes in the payload could close the paste early or act as
    // keystrokes (ESC, ^C…) — only text, newlines and tabs go through
    let clean: String = text
        .chars()
        .filter(|&c| !c.is_control() || matches!(c, '\n' | '\t' | '\r'))
        .collect();
    let step = |data: String, delay_ms| PtyWriteStep { data, delay_ms };
    // Shells: clear any half-typed line first (^U)
    let prefix = if is_shell { "\x15" } else { "" };
    if bracketed {
        // Give the TUI a beat to finish ingesting a large paste before Enter
        let enter_delay = if is_shell { 30 } else { 120 + (clean.len() as u64 / 40).min(380) };
        vec![
            step(format!("{prefix}\x1b[200~{clean}\x1b[201~"), 0),
            step("\r".into(), enter_delay),
        ]
    } else if is_shell {
        vec![step(format!("{prefix}{clean}\r"), 0)]
    } else {
        // Without paste mode a newline would submit early — flatten it
        let flat = clean.replace("\r\n", " ").replace(['\n', '\r'], " ");
        vec![step(flat, 0), step("\r".into(), 300)]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LONG: Duration = Duration::from_secs(5);
    const SHORT: Duration = Duration::from_millis(200);

    #[test]
    fn model_renders_and_tracks_bracketed_paste() {
        let mut m = ScreenModel::new(5, 40);
        assert!(!m.bracketed_paste());
        m.feed(b"\x1b[?2004hhello\r\n\x1b[31mworld\x1b[0m");
        assert!(m.bracketed_paste());
        assert_eq!(m.text(), "hello\nworld");
        m.feed(b"\x1b[?2004l");
        assert!(!m.bracketed_paste());
    }

    #[test]
    fn redraw_with_same_content_is_not_a_change() {
        let mut m = ScreenModel::new(3, 20);
        m.feed(b"idle prompt");
        let t1 = Instant::now();
        m.refresh(t1);
        let first = m.last_change;
        // Same text redrawn in place (cursor home + rewrite)
        m.feed(b"\r\x1b[2Kidle prompt");
        m.refresh(t1 + Duration::from_secs(1));
        assert_eq!(m.last_change, first);
        m.feed(b"!");
        let t3 = t1 + Duration::from_secs(2);
        m.refresh(t3);
        assert_eq!(m.last_change, t3);
    }

    #[test]
    fn spinner_footer_is_working_until_stale() {
        let f = footer_of("answer\n✻ Thinking… (12s · esc to interrupt)\n> \n? for shortcuts");
        assert_eq!(classify(&f, "", Duration::from_secs(3), false, None), TurnState::Working);
        assert_eq!(classify(&f, "", Duration::from_secs(30), false, None), TurnState::Idle);
    }

    #[test]
    fn settle_window_keeps_agent_working() {
        let f = footer_of("> \n? for shortcuts");
        assert_eq!(classify(&f, "", SHORT, false, None), TurnState::Working);
        assert_eq!(classify(&f, "", LONG, false, None), TurnState::Idle);
    }

    #[test]
    fn permission_menus_need_attention() {
        let claude = "Do you want to proceed?\n❯ 1. Yes\n  2. Yes, and don't ask again\n  3. No, and tell Claude what to do differently (esc)";
        assert_eq!(classify(&footer_of(claude), "", LONG, false, None), TurnState::Attention);
        let codex = "Would you like to run the following command?\n› 1. Yes, proceed (y)\n  2. No (esc)";
        assert_eq!(classify(&footer_of(codex), "", LONG, false, None), TurnState::Attention);
        let gemini = "Allow execution?\n● 1. Allow once\n  2. Allow always";
        assert_eq!(classify(&footer_of(gemini), "", LONG, false, None), TurnState::Attention);
        assert_eq!(classify(&footer_of("Overwrite file? (y/n)"), "", LONG, false, None), TurnState::Attention);
    }

    #[test]
    fn plain_question_in_an_answer_is_idle() {
        let f = footer_of("I fixed the bug.\nWould you like me to also add tests?\n> \n? for shortcuts");
        assert_eq!(classify(&f, "", LONG, false, None), TurnState::Idle);
    }

    #[test]
    fn shell_busy_comes_from_foreground_job() {
        assert_eq!(classify("$", "", LONG, true, Some(true)), TurnState::Working);
        assert_eq!(classify("$", "", LONG, true, Some(false)), TurnState::Idle);
        assert_eq!(classify("$", "", SHORT, true, Some(false)), TurnState::Working);
    }

    #[test]
    fn delivery_uses_paste_only_when_enabled() {
        let p = plan_delivery("line1\nline2", true, false);
        assert_eq!(p[0].data, "\x1b[200~line1\nline2\x1b[201~");
        assert_eq!(p[1].data, "\r");
        assert!(p[1].delay_ms > 0);

        let p = plan_delivery("line1\nline2", false, false);
        assert_eq!(p[0].data, "line1 line2");
        assert_eq!(p[1], PtyWriteStep { data: "\r".into(), delay_ms: 300 });

        let p = plan_delivery("ls -la", false, true);
        assert_eq!(p, vec![PtyWriteStep { data: "\x15ls -la\r".into(), delay_ms: 0 }]);

        let p = plan_delivery("ls", true, true);
        assert_eq!(p[0].data, "\x15\x1b[200~ls\x1b[201~");
    }

    #[test]
    fn delivery_strips_escape_injection() {
        let p = plan_delivery("evil\x1b[201~\x03", true, false);
        assert_eq!(p[0].data, "\x1b[200~evil[201~\x1b[201~");
    }

    #[test]
    fn trim_screen_drops_trailing_blank_rows() {
        assert_eq!(trim_screen("a  \nb\n\n   \n"), "a\nb");
        assert_eq!(trim_screen("\n\n"), "");
    }

    #[test]
    fn title_glyphs_follow_the_catalog() {
        assert_eq!(title_signal("✳ PONG reply test"), TitleSignal::Idle);
        assert_eq!(title_signal("◐ PONG reply test"), TitleSignal::Working);
        assert_eq!(title_signal("⠋ codex"), TitleSignal::Working);
        assert_eq!(title_signal("✋ gemini"), TitleSignal::Attention);
        assert_eq!(title_signal("◇ gemini"), TitleSignal::Idle);
        assert_eq!(title_signal("bash"), TitleSignal::Unknown);
        assert_eq!(title_signal(""), TitleSignal::Unknown);
    }

    #[test]
    fn an_idle_title_settles_faster_than_the_screen_alone() {
        let ms = Duration::from_millis;
        // Claude at rest: idle after TITLE_SETTLE, not SETTLE
        assert_eq!(classify("? for shortcuts", "✳ Task", ms(300), false, None), TurnState::Working);
        assert_eq!(classify("? for shortcuts", "✳ Task", ms(500), false, None), TurnState::Idle);
        assert_eq!(classify("? for shortcuts", "", ms(500), false, None), TurnState::Working);
        // A busy footer wins over a resting title
        assert_eq!(classify("esc to interrupt", "✳ Task", ms(500), false, None), TurnState::Working);
        // A spinning title keeps it working past SETTLE, until stale
        assert_eq!(classify("", "◐ Task", Duration::from_secs(5), false, None), TurnState::Working);
        assert_eq!(classify("", "◐ Task", Duration::from_secs(30), false, None), TurnState::Idle);
        // A permission menu under a resting title is attention at once, even
        // with a bullet blinking (the screen changed 100 ms ago)
        let menu = footer_of("Do you want to proceed?\n❯ 1. Yes\n  2. No\nEsc to cancel · Tab to amend");
        assert_eq!(classify(&menu, "✳ Task", ms(100), false, None), TurnState::Attention);
        // Without the title, the menu still needs a quiet screen
        assert_eq!(classify(&menu, "", ms(100), false, None), TurnState::Working);
    }

    #[test]
    fn title_changes_count_as_activity() {
        let mut m = ScreenModel::new(3, 20);
        m.feed(b"same");
        let t1 = Instant::now();
        m.refresh(t1);
        m.feed(b"\x1b]0;\xe2\x97\x90 Task\x07");
        let t2 = t1 + Duration::from_secs(1);
        m.refresh(t2);
        assert_eq!(m.last_change, t2);
        assert_eq!(m.title(), "◐ Task");
    }
}
