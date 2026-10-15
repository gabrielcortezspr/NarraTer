//! The answer of an `ask`, read from the agent's own transcript (backlog O5).
//!
//! The screen shows ~13 rows; a worker's report is often longer, so the lead
//! got it cut and had to ask again (benchmark of 2026-10-15,
//! docs/pesquisa/comparacao-versoes.md). Claude Code runs on the alternate
//! screen, so there is no scrollback to read either (docs/pesquisa/orca.md
//! §7.2). But it writes every turn to a JSON Lines transcript, which we can
//! read without the agent doing anything (D01): this is an optional
//! improvement, and the screen stays the fallback (D05).
//!
//! At delivery we remember the size of every transcript in the agent's project
//! folder (and of the one the status line reported, D17). At the end of the
//! turn, the file that received our prompt after that offset is the agent's;
//! its last assistant text after the last tool call is the answer.

use std::collections::HashMap;
use std::fs;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

/// Longest answer returned whole; longer ones keep the end (the conclusion).
pub const MAX_ANSWER_CHARS: usize = 20_000;
/// How far we read past the delivery offset (a long turn with big tool results).
const MAX_SCAN_BYTES: u64 = 16 * 1024 * 1024;

/// Where Claude Code keeps transcripts for a working directory:
/// `<config>/projects/<cwd with every non-alphanumeric char as '-'>/`.
pub fn claude_project_dir(cwd: &Path) -> Option<PathBuf> {
    let config = std::env::var_os("CLAUDE_CONFIG_DIR")
        .map(PathBuf::from)
        .or_else(|| dirs::home_dir().map(|h| h.join(".claude")))?;
    let escaped: String = cwd
        .to_string_lossy()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    Some(config.join("projects").join(escaped))
}

/// The transcripts that existed when the prompt went in, and their sizes.
#[derive(Debug, Clone, Default)]
pub struct Anchor {
    files: HashMap<PathBuf, u64>,
    dirs: Vec<PathBuf>,
}

impl Anchor {
    /// `dir`: the agent's project folder in Claude's store; `hint`: the
    /// transcript the status line reported for this terminal, if any.
    pub fn take(dir: Option<&Path>, hint: Option<&Path>) -> Anchor {
        let mut a = Anchor::default();
        if let Some(d) = dir {
            a.dirs.push(d.to_path_buf());
            if let Ok(entries) = fs::read_dir(d) {
                for e in entries.flatten() {
                    let p = e.path();
                    if p.extension().is_some_and(|x| x == "jsonl") {
                        a.files.insert(p, e.metadata().map(|m| m.len()).unwrap_or(0));
                    }
                }
            }
        }
        if let Some(h) = hint {
            let len = fs::metadata(h).map(|m| m.len()).unwrap_or(0);
            a.files.entry(h.to_path_buf()).or_insert(len);
            if let Some(parent) = h.parent() {
                if !a.dirs.iter().any(|d| d == parent) {
                    a.dirs.push(parent.to_path_buf());
                }
            }
        }
        a
    }

    /// Files that grew (or appeared) since the anchor, with the offset to read from.
    fn candidates(&self) -> Vec<(PathBuf, u64)> {
        let mut out = Vec::new();
        for d in &self.dirs {
            let Ok(entries) = fs::read_dir(d) else { continue };
            for e in entries.flatten() {
                let p = e.path();
                if !p.extension().is_some_and(|x| x == "jsonl") {
                    continue;
                }
                let len = e.metadata().map(|m| m.len()).unwrap_or(0);
                let from = self.files.get(&p).copied().unwrap_or(0);
                if len > from {
                    out.push((p, from));
                }
            }
        }
        out
    }
}

/// What the transcript says about the turn that answers our prompt.
#[derive(Debug, PartialEq)]
pub enum Found {
    /// The agent's final answer for our prompt.
    Answer(String),
    /// Our prompt is there and the turn hasn't ended: the agent is still
    /// working, whatever the screen says.
    Pending,
    /// No transcript received our prompt.
    Missing,
}

/// The final answer to the prompt that contains `needle`, from the files that
/// grew since `anchor`.
pub fn final_answer(anchor: &Anchor, msg: &str) -> Found {
    let needle = needle_of(msg);
    if needle.is_empty() {
        return Found::Missing;
    }
    let mut pending = false;
    for (path, from) in anchor.candidates() {
        let Some(text) = read_from(&path, from) else { continue };
        match answer_in(&text, &needle) {
            Found::Answer(a) => return Found::Answer(a),
            Found::Pending => pending = true,
            Found::Missing => {}
        }
    }
    if pending { Found::Pending } else { Found::Missing }
}

fn read_from(path: &Path, from: u64) -> Option<String> {
    let mut f = fs::File::open(path).ok()?;
    f.seek(SeekFrom::Start(from)).ok()?;
    let mut buf = Vec::new();
    f.take(MAX_SCAN_BYTES).read_to_end(&mut buf).ok()?;
    // A partial last line (still being written) is dropped by the JSON parse
    Some(String::from_utf8_lossy(&buf).into_owned())
}

/// Whitespace collapsed: how we compare our prompt with the transcript's copy
/// (the paste may re-wrap it).
fn squash(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// What identifies our prompt: the start of the message we delivered.
fn needle_of(msg: &str) -> String {
    squash(msg).chars().take(100).collect()
}

/// A user record that is a typed/pasted prompt (not a tool result) and its text.
fn prompt_text(v: &serde_json::Value) -> Option<String> {
    if v.get("type")?.as_str()? != "user" || v.get("isMeta").and_then(|m| m.as_bool()) == Some(true) {
        return None;
    }
    match v.pointer("/message/content")? {
        serde_json::Value::String(s) => Some(s.clone()),
        serde_json::Value::Array(blocks) => {
            if blocks.iter().any(|b| b.get("type").and_then(|t| t.as_str()) == Some("tool_result")) {
                return None;
            }
            let t: Vec<&str> = blocks.iter().filter_map(|b| b.get("text").and_then(|t| t.as_str())).collect();
            (!t.is_empty()).then(|| t.join("\n"))
        }
        _ => None,
    }
}

/// The turn that answers the prompt containing `needle`: the assistant text
/// written after the turn's last tool call. Pending until the record that
/// ends the turn (`stop_reason: end_turn`) is there.
pub fn answer_in(jsonl: &str, needle: &str) -> Found {
    let records: Vec<serde_json::Value> = jsonl.lines().filter_map(|l| serde_json::from_str(l).ok()).collect();
    let Some(start) = records
        .iter()
        .position(|v| prompt_text(v).is_some_and(|t| squash(&t).contains(needle)))
    else {
        return Found::Missing;
    };
    let mut texts: Vec<String> = Vec::new();
    let mut ended = false;
    for v in &records[start + 1..] {
        if prompt_text(v).is_some() {
            // The next prompt (or an interruption): our turn is over
            ended = true;
            break;
        }
        // Claude Code closes every turn with this record
        if v.get("type").and_then(|t| t.as_str()) == Some("system")
            && v.get("subtype").and_then(|t| t.as_str()) == Some("turn_duration")
        {
            ended = true;
            break;
        }
        if v.get("type").and_then(|t| t.as_str()) != Some("assistant") {
            continue;
        }
        let Some(blocks) = v.pointer("/message/content").and_then(|c| c.as_array()) else { continue };
        let mut has_text = false;
        for b in blocks {
            match b.get("type").and_then(|t| t.as_str()) {
                Some("tool_use") => texts.clear(), // narration before a tool isn't the answer
                Some("text") => {
                    if let Some(t) = b.get("text").and_then(|t| t.as_str()) {
                        if !t.trim().is_empty() {
                            texts.push(t.trim_end().to_string());
                            has_text = true;
                        }
                    }
                }
                _ => {}
            }
        }
        // Each block is its own record and all carry the message's stop
        // reason: the thinking block says end_turn ~15 s before the text is
        // written (real case, benchmark E). Only a text block with end_turn
        // counts, for versions that don't write turn_duration.
        if has_text && v.pointer("/message/stop_reason").and_then(|s| s.as_str()) == Some("end_turn") {
            ended = true;
        }
    }
    if !ended {
        return Found::Pending;
    }
    if texts.is_empty() {
        // Over, but with nothing to say (interrupted): the screen tells more
        return Found::Missing;
    }
    // Each block may be written several times while streaming: keep the last copy of each
    texts.dedup();
    let answer = texts.join("\n\n");
    Found::Answer(clip(&answer))
}

fn clip(s: &str) -> String {
    let n = s.chars().count();
    if n <= MAX_ANSWER_CHARS {
        return s.to_string();
    }
    let tail: String = s.chars().skip(n - MAX_ANSWER_CHARS).collect();
    format!("[… {} characters omitted from the start]\n{}", n - MAX_ANSWER_CHARS, tail)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn user(text: &str) -> String {
        serde_json::json!({"type": "user", "message": {"role": "user", "content": text}}).to_string()
    }
    fn tool_result() -> String {
        serde_json::json!({"type": "user", "message": {"role": "user", "content": [{"type": "tool_result", "tool_use_id": "t1", "content": "ok"}]}}).to_string()
    }
    fn assistant(id: &str, blocks: serde_json::Value, stop: Option<&str>) -> String {
        serde_json::json!({"type": "assistant", "message": {"id": id, "role": "assistant", "content": blocks, "stop_reason": stop}}).to_string()
    }

    #[test]
    fn the_answer_is_the_text_after_the_last_tool_call() {
        let t = [
            user("[narrater from Narrador]: rode os testes e me diga o resultado"),
            assistant("m1", serde_json::json!([{"type": "text", "text": "Vou rodar os testes."}, {"type": "tool_use", "id": "t1", "name": "Bash", "input": {}}]), Some("tool_use")),
            tool_result(),
            assistant("m2", serde_json::json!([{"type": "text", "text": "Ran 44 tests\n\nOK"}]), Some("end_turn")),
        ]
        .join("\n");
        assert_eq!(answer_in(&t, &needle_of("rode os testes e me diga o resultado")), Found::Answer("Ran 44 tests\n\nOK".into()));
    }

    #[test]
    fn a_turn_without_its_last_record_is_pending() {
        let t = [
            user("[narrater from Narrador]: faça X"),
            assistant("m1", serde_json::json!([{"type": "tool_use", "id": "t1", "name": "Bash", "input": {}}]), Some("tool_use")),
        ]
        .join("\n");
        assert_eq!(answer_in(&t, &needle_of("faça X")), Found::Pending);
    }

    #[test]
    fn another_prompt_is_missing_and_the_next_prompt_ends_the_turn() {
        let t = [
            user("[narrater from Narrador]: primeira"),
            assistant("m1", serde_json::json!([{"type": "text", "text": "resposta 1"}]), Some("end_turn")),
            user("[narrater from Narrador]: segunda"),
            assistant("m2", serde_json::json!([{"type": "text", "text": "resposta 2"}]), Some("end_turn")),
        ]
        .join("\n");
        assert_eq!(answer_in(&t, &needle_of("primeira")), Found::Answer("resposta 1".into()));
        assert_eq!(answer_in(&t, &needle_of("terceira")), Found::Missing);
    }

    #[test]
    fn long_answers_keep_the_end() {
        let long = "a".repeat(MAX_ANSWER_CHARS + 10) + "FIM";
        let c = clip(&long);
        assert!(c.starts_with("[… 13 characters omitted"));
        assert!(c.ends_with("FIM"));
    }

    #[test]
    fn the_anchor_reads_only_what_came_after_delivery() {
        let dir = std::env::temp_dir().join(format!("narrater-transcript-{}", std::process::id()));
        let _ = fs::create_dir_all(&dir);
        let f = dir.join("s.jsonl");
        // Same prompt text before the anchor: must not be taken as our turn
        fs::write(&f, [user("[narrater from Narrador]: status?"), assistant("m0", serde_json::json!([{"type": "text", "text": "antigo"}]), Some("end_turn"))].join("\n") + "\n").unwrap();
        let anchor = Anchor::take(Some(&dir), None);
        let mut more = fs::OpenOptions::new().append(true).open(&f).unwrap();
        use std::io::Write;
        writeln!(more, "{}", user("[narrater from Narrador]: status?")).unwrap();
        writeln!(more, "{}", assistant("m1", serde_json::json!([{"type": "text", "text": "novo"}]), Some("end_turn"))).unwrap();
        assert_eq!(final_answer(&anchor, "status?"), Found::Answer("novo".into()));
        let _ = fs::remove_dir_all(&dir);
    }

    /// Real case (benchmark E, round 2): the screen said DevA was idle at
    /// 15:40:37.7, 5 s after delivery; a thinking block with end_turn came at
    /// 15:40:47.7 and the answer text only at 15:41:03.1 (turn_duration right
    /// after). Pending at both earlier moments; whole, the answer.
    #[test]
    fn a_real_premature_idle_is_pending_in_the_transcript() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/transcripts/claude-2.1.300-ask-prematuro.jsonl");
        let full = fs::read_to_string(path).unwrap();
        let needle = needle_of("Projeto: /home/user/bench/runs/vE-r2/work/pedidos. NÃO altere nada ainda.");
        let at_premature: String = full
            .lines()
            .filter(|l| {
                let v: serde_json::Value = serde_json::from_str(l).unwrap();
                v["timestamp"].as_str().unwrap() <= "2026-10-15T15:40:37.731Z"
            })
            .collect::<Vec<_>>()
            .join("\n");
        assert_eq!(answer_in(&at_premature, &needle), Found::Pending);
        let at_thinking: String = full
            .lines()
            .filter(|l| {
                let v: serde_json::Value = serde_json::from_str(l).unwrap();
                v["timestamp"].as_str().unwrap() <= "2026-10-15T15:40:50.000Z"
            })
            .collect::<Vec<_>>()
            .join("\n");
        assert_eq!(answer_in(&at_thinking, &needle), Found::Pending);
        match answer_in(&full, &needle) {
            Found::Answer(a) => assert!(a.len() > 200, "a resposta inteira: {a}"),
            other => panic!("esperava a resposta, veio {other:?}"),
        }
    }

    #[test]
    fn an_interrupted_turn_falls_back_to_the_screen() {
        let t = [
            user("[narrater from Narrador]: faça X"),
            assistant("m1", serde_json::json!([{"type": "tool_use", "id": "t1", "name": "Bash", "input": {}}]), Some("tool_use")),
            user("[Request interrupted by user]"),
        ]
        .join("\n");
        assert_eq!(answer_in(&t, &needle_of("faça X")), Found::Missing);
    }

    #[test]
    fn project_dir_escapes_like_claude_code() {
        let d = claude_project_dir(Path::new("/home/user/proj/.narrater/roles/coder")).unwrap();
        assert!(d.ends_with("projects/-home-user-proj--narrater-roles-coder"));
    }
}
