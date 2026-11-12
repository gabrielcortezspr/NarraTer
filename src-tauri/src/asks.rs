//! Asks as durable records. Every `ask` gets an id and runs in the background;
//! its outcome lands here. That lets a caller fire several asks and `wait` for
//! whichever finishes first, or come back to one after its own shell timed
//! out, instead of re-sending it.
//!
//! Persisted as JSON lines (one line per state change, the last line of an id
//! wins), compacted on load. An ask that was still in flight when the app
//! stopped comes back as `unknown`: its target may well have finished, but
//! nothing observed it, so nothing claims it did.

use std::collections::HashMap;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tokio::sync::Notify;

/// Records kept on compaction (newest first).
const KEEP: usize = 500;
/// Appended lines past this many records trigger a compaction.
const COMPACT_AFTER: usize = 4 * KEEP;

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum AskState {
    /// Waiting in the target's queue.
    Queued,
    /// Typed in; the target's turn is running.
    Delivered,
    /// The target's turn ended; `result` is its screen.
    Done,
    /// The target stopped at a permission menu or y/n prompt.
    Attention,
    /// The turn ended but the acceptance check (`ask --check`) failed.
    Failed,
    /// The target messaged the asker back instead (ask-back).
    Answered,
    /// Never delivered, or the turn outlasted the timeout.
    Timeout,
    /// The target (or the asker) went away first.
    Exited,
    /// Removed from the queue by the user before delivery.
    Cancelled,
    /// In flight when the app stopped: the outcome was never observed.
    Unknown,
}

impl AskState {
    pub fn finished(self) -> bool {
        !matches!(self, AskState::Queued | AskState::Delivered)
    }

    pub fn as_str(self) -> &'static str {
        match self {
            AskState::Queued => "queued",
            AskState::Delivered => "delivered",
            AskState::Done => "done",
            AskState::Attention => "attention",
            AskState::Failed => "failed",
            AskState::Answered => "answered",
            AskState::Timeout => "timeout",
            AskState::Exited => "exited",
            AskState::Cancelled => "cancelled",
            AskState::Unknown => "unknown",
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AskRecord {
    pub id: String,
    /// Asker and target node ids.
    pub from: String,
    pub to: String,
    pub to_label: String,
    pub prompt: String,
    pub state: AskState,
    /// What the asker is shown: the target's screen, or the error/notice.
    #[serde(default)]
    pub result: Option<String>,
    /// Epoch milliseconds.
    pub created: u64,
    #[serde(default)]
    pub delivered: Option<u64>,
    #[serde(default)]
    pub finished: Option<u64>,
    /// Started with `--no-wait`: a bare `narrater wait` collects it.
    #[serde(default)]
    pub detached: bool,
    /// Its outcome was handed to the asker (by `ask` or `wait`).
    #[serde(default)]
    pub reported: bool,
    /// An earlier ask from the same asker to the same target with (almost) the
    /// same prompt: the lead is repeating itself (MAST's most common failure).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repeat_of: Option<String>,
}

/// How far back a repeated prompt is looked for.
pub const REPEAT_WINDOW_MS: u64 = 60 * 60 * 1000;

/// Words of a prompt, lowercased, for comparing two prompts.
fn words(s: &str) -> std::collections::HashSet<String> {
    s.split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()).map(|w| w.to_lowercase()).collect()
}

/// Two prompts say the same thing: identical once whitespace and case are
/// ignored, or (from 4 words up) sharing at least 90% of their words.
pub fn same_prompt(a: &str, b: &str) -> bool {
    let squash = |s: &str| s.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase();
    if squash(a) == squash(b) {
        return true;
    }
    let (wa, wb) = (words(a), words(b));
    if wa.len() < 4 || wb.len() < 4 {
        return false;
    }
    let common = wa.intersection(&wb).count() as f64;
    common / (wa.union(&wb).count() as f64) >= 0.9
}

#[derive(Default)]
struct Inner {
    records: HashMap<String, AskRecord>,
    /// Lines appended since the last compaction.
    appended: usize,
}

/// The ask records. `path: None` keeps them in memory only (tests).
#[derive(Default)]
pub struct AskStore {
    inner: Mutex<Inner>,
    path: Option<PathBuf>,
    changed: Notify,
}

pub fn default_path() -> PathBuf {
    dirs::config_dir().unwrap_or_else(|| PathBuf::from(".")).join("narrater/asks.jsonl")
}

pub fn new_id() -> String {
    format!("ask-{}", &uuid::Uuid::new_v4().simple().to_string()[..6])
}

/// What a `wait` returns.
pub enum WaitOutcome {
    /// Finished asks (one for `--any`, all of them for `--all`).
    Finished(Vec<AskRecord>),
    /// Nothing finished in time; these are still running.
    Pending(Vec<AskRecord>),
    /// Nothing to wait for.
    Nothing,
}

impl AskStore {
    pub fn load(path: PathBuf) -> Self {
        let mut records: HashMap<String, AskRecord> = HashMap::new();
        if let Ok(text) = std::fs::read_to_string(&path) {
            for line in text.lines() {
                if let Ok(r) = serde_json::from_str::<AskRecord>(line) {
                    records.insert(r.id.clone(), r);
                }
            }
        }
        for r in records.values_mut() {
            if !r.state.finished() {
                r.state = AskState::Unknown;
                r.result = Some(format!(
                    "Error: NarraTer restarted while this ask to '{}' was {} — its outcome is unknown. Run `narrater check \"{}\"` to see its screen; do NOT re-send.",
                    r.to_label,
                    if r.delivered.is_some() { "running" } else { "queued" },
                    r.to_label
                ));
                r.finished = Some(crate::pty::now_ms());
            }
        }
        let store = AskStore { inner: Mutex::new(Inner { records, appended: 0 }), path: Some(path), changed: Notify::new() };
        store.compact();
        store
    }

    /// Rewrites the file with the newest KEEP records.
    fn compact(&self) {
        let mut inner = self.inner.lock().unwrap();
        if inner.records.len() > KEEP {
            let mut all: Vec<AskRecord> = inner.records.values().cloned().collect();
            all.sort_by(|a, b| b.created.cmp(&a.created));
            for r in all.into_iter().skip(KEEP) {
                inner.records.remove(&r.id);
            }
        }
        inner.appended = 0;
        let Some(path) = &self.path else { return };
        let mut all: Vec<&AskRecord> = inner.records.values().collect();
        all.sort_by_key(|r| r.created);
        let body: String = all.iter().filter_map(|r| serde_json::to_string(r).ok()).map(|l| l + "\n").collect();
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let tmp = path.with_extension("jsonl.tmp");
        if std::fs::write(&tmp, body).is_ok() {
            let _ = std::fs::rename(&tmp, path);
        }
    }

    fn append(&self, inner: &mut Inner, r: &AskRecord) {
        let Some(path) = &self.path else { return };
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let (Ok(mut f), Ok(line)) =
            (std::fs::OpenOptions::new().create(true).append(true).open(path), serde_json::to_string(r))
        {
            let _ = writeln!(f, "{}", line);
        }
        inner.appended += 1;
    }

    pub fn insert(&self, r: AskRecord) {
        let compact = {
            let mut inner = self.inner.lock().unwrap();
            self.append(&mut inner, &r);
            inner.records.insert(r.id.clone(), r);
            inner.appended > COMPACT_AFTER
        };
        if compact {
            self.compact();
        }
        self.changed.notify_waiters();
    }

    /// Applies `f` to the record and persists it; a no-op for unknown ids.
    pub fn update(&self, id: &str, f: impl FnOnce(&mut AskRecord)) {
        {
            let mut inner = self.inner.lock().unwrap();
            let Some(r) = inner.records.get_mut(id) else { return };
            f(r);
            let r = r.clone();
            self.append(&mut inner, &r);
        }
        self.changed.notify_waiters();
    }

    /// Marks the ask delivered (typed into the target).
    pub fn delivered(&self, id: &str) {
        self.update(id, |r| {
            r.state = AskState::Delivered;
            r.delivered = Some(crate::pty::now_ms());
        });
    }

    /// Records the outcome. Never overwrites one already recorded.
    pub fn finish(&self, id: &str, state: AskState, result: String) {
        self.update(id, |r| {
            if r.state.finished() {
                return;
            }
            r.state = state;
            r.result = Some(result);
            r.finished = Some(crate::pty::now_ms());
        });
    }

    pub fn get(&self, id: &str) -> Option<AskRecord> {
        self.inner.lock().unwrap().records.get(id).cloned()
    }

    /// The newest earlier ask from `asker` to `target` with the same prompt,
    /// within REPEAT_WINDOW_MS of `now`.
    pub fn repeat_of(&self, asker: &str, target: &str, prompt: &str, now: u64) -> Option<AskRecord> {
        self.of(asker)
            .into_iter()
            .filter(|r| r.to == target && now.saturating_sub(r.created) <= REPEAT_WINDOW_MS)
            .find(|r| same_prompt(&r.prompt, prompt))
    }

    /// The asker's records, newest first.
    pub fn of(&self, asker: &str) -> Vec<AskRecord> {
        let inner = self.inner.lock().unwrap();
        let mut v: Vec<AskRecord> = inner.records.values().filter(|r| r.from == asker).cloned().collect();
        v.sort_by(|a, b| b.created.cmp(&a.created));
        v
    }

    fn mark_reported(&self, ids: &[String]) {
        let mut inner = self.inner.lock().unwrap();
        for id in ids {
            let changed = match inner.records.get_mut(id) {
                Some(r) if !r.reported => {
                    r.reported = true;
                    Some(r.clone())
                }
                _ => None,
            };
            if let Some(r) = changed {
                self.append(&mut inner, &r);
            }
        }
    }

    /// Waits for `asker`'s asks. With `ids` empty, it waits on its detached
    /// asks that weren't collected yet. `all`: until every one finished;
    /// otherwise until the first does (an already finished, uncollected one
    /// is returned at once). Ids are checked to belong to the asker.
    pub async fn wait(&self, asker: &str, ids: &[String], all: bool, timeout: Duration) -> Result<WaitOutcome, String> {
        for id in ids {
            match self.get(id) {
                Some(r) if r.from == asker => {}
                _ => return Err(format!("Error: no ask '{}' of yours — `narrater asks` lists them", id)),
            }
        }
        let deadline = tokio::time::Instant::now() + timeout;
        loop {
            let notified = self.changed.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();

            let watched: Vec<AskRecord> = if ids.is_empty() {
                self.of(asker).into_iter().filter(|r| r.detached && !r.reported).collect()
            } else {
                ids.iter().filter_map(|id| self.get(id)).collect()
            };
            if watched.is_empty() {
                return Ok(WaitOutcome::Nothing);
            }
            let (mut done, pending): (Vec<AskRecord>, Vec<AskRecord>) =
                watched.into_iter().partition(|r| r.state.finished());
            if all {
                if pending.is_empty() {
                    self.mark_reported(&done.iter().map(|r| r.id.clone()).collect::<Vec<_>>());
                    return Ok(WaitOutcome::Finished(done));
                }
            } else if !done.is_empty() {
                // Uncollected first, then the one that finished first
                done.sort_by_key(|r| (r.reported, r.finished.unwrap_or(0)));
                let first = done.swap_remove(0);
                self.mark_reported(&[first.id.clone()]);
                return Ok(WaitOutcome::Finished(vec![first]));
            }
            tokio::select! {
                _ = &mut notified => {}
                _ = tokio::time::sleep_until(deadline) => return Ok(WaitOutcome::Pending(pending)),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn rec(id: &str, from: &str, detached: bool) -> AskRecord {
        AskRecord {
            id: id.into(),
            from: from.into(),
            to: "t".into(),
            to_label: "Dev".into(),
            prompt: "do it".into(),
            state: AskState::Queued,
            result: None,
            created: crate::pty::now_ms(),
            delivered: None,
            finished: None,
            detached,
            reported: false,
            repeat_of: None,
        }
    }

    #[test]
    fn same_prompt_ignores_spacing_case_and_small_edits() {
        assert!(same_prompt("Implement the  cubic weight", "implement the cubic weight"));
        // One extra word in 17: still the same request
        assert!(same_prompt(
            "implement docs/PEDIDOS.md section 1 in armazem/frete.py with tests and run them all",
            "implement docs/PEDIDOS.md section 1 in armazem/frete.py with tests, and run them all please"
        ));
        assert!(!same_prompt("fix frete", "fix cupons"));
        assert!(!same_prompt("run the tests", "run the tests again now"));
    }

    #[test]
    fn repeat_is_same_asker_same_target_recent() {
        let dir = std::env::temp_dir().join(format!("asks-rep-{}", new_id()));
        let store = AskStore::load(dir.join("asks.jsonl"));
        let mut a = rec("ask-1", "lead", false);
        a.prompt = "Implement the cubic weight in frete.py".into();
        store.insert(a);
        let now = crate::pty::now_ms();
        assert_eq!(store.repeat_of("lead", "t", "implement the cubic  weight in frete.py", now).map(|r| r.id), Some("ask-1".into()));
        assert!(store.repeat_of("other", "t", "Implement the cubic weight in frete.py", now).is_none());
        assert!(store.repeat_of("lead", "t2", "Implement the cubic weight in frete.py", now).is_none());
        assert!(store.repeat_of("lead", "t", "Implement the cubic weight in frete.py", now + REPEAT_WINDOW_MS + 1).is_none());
        let _ = std::fs::remove_dir_all(dir);
    }

    fn temp_path(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("narrater-asks-test-{}-{}", name, std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir.join("asks.jsonl")
    }

    #[test]
    fn in_flight_asks_come_back_unknown_after_a_restart() {
        let path = temp_path("restart");
        {
            let s = AskStore::load(path.clone());
            s.insert(rec("a1", "me", true));
            s.delivered("a1");
            s.insert(rec("a2", "me", false));
            s.finish("a2", AskState::Done, "screen".into());
        }
        let s = AskStore::load(path.clone());
        let a1 = s.get("a1").unwrap();
        assert_eq!(a1.state, AskState::Unknown);
        assert!(a1.result.unwrap().contains("running"));
        assert_eq!(s.get("a2").unwrap().result.as_deref(), Some("screen"));
        // compacted: one line per record
        assert_eq!(std::fs::read_to_string(&path).unwrap().lines().count(), 2);
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn an_outcome_is_never_overwritten() {
        let s = AskStore::default();
        s.insert(rec("a", "me", false));
        s.finish("a", AskState::Answered, "back".into());
        s.finish("a", AskState::Exited, "gone".into());
        assert_eq!(s.get("a").unwrap().state, AskState::Answered);
    }

    #[tokio::test]
    async fn wait_any_returns_the_first_to_finish_and_collects_it() {
        let s = Arc::new(AskStore::default());
        s.insert(rec("a", "me", true));
        s.insert(rec("b", "me", true));
        let s2 = Arc::clone(&s);
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(30)).await;
            s2.finish("b", AskState::Done, "B".into());
        });
        let WaitOutcome::Finished(v) = s.wait("me", &[], false, Duration::from_secs(5)).await.unwrap() else {
            panic!("expected a finished ask")
        };
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].id, "b");
        // collected: a bare wait now only watches "a"
        let WaitOutcome::Pending(p) = s.wait("me", &[], false, Duration::from_millis(20)).await.unwrap() else {
            panic!("expected pending")
        };
        assert_eq!(p.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(), vec!["a"]);
    }

    #[tokio::test]
    async fn wait_all_and_explicit_ids() {
        let s = AskStore::default();
        s.insert(rec("a", "me", false));
        s.insert(rec("b", "me", false));
        s.finish("a", AskState::Done, "A".into());
        let ids = vec!["a".to_string(), "b".to_string()];
        assert!(matches!(s.wait("me", &ids, true, Duration::from_millis(20)).await.unwrap(), WaitOutcome::Pending(_)));
        s.finish("b", AskState::Timeout, "late".into());
        let WaitOutcome::Finished(v) = s.wait("me", &ids, true, Duration::from_secs(1)).await.unwrap() else {
            panic!("expected both")
        };
        assert_eq!(v.len(), 2);
        // a non-detached (synchronous) ask is not collected by a bare wait
        assert!(matches!(s.wait("me", &[], false, Duration::from_millis(5)).await.unwrap(), WaitOutcome::Nothing));
        // someone else's ask is refused
        assert!(s.wait("other", &ids, false, Duration::from_millis(5)).await.is_err());
    }

    /// Bancada da D07: `cargo test --release asks_bench -- --ignored --nocapture`.
    /// Pior caso: 2000 linhas (o limite antes de compactar) com respostas de 8 KB.
    #[test]
    #[ignore]
    fn asks_bench() {
        let path = temp_path("bench");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let screen = "x".repeat(8 * 1024);
        let mut body = String::new();
        for i in 0..2000 {
            let mut r = rec(&format!("ask-{:06}", i), "me", true);
            r.state = AskState::Done;
            r.result = Some(screen.clone());
            body.push_str(&serde_json::to_string(&r).unwrap());
            body.push('\n');
        }
        std::fs::write(&path, &body).unwrap();
        let t = std::time::Instant::now();
        let s = AskStore::load(path.clone());
        let load = t.elapsed();
        let size_after = std::fs::metadata(&path).unwrap().len();
        let t = std::time::Instant::now();
        for i in 0..100 {
            s.insert(rec(&format!("new-{}", i), "me", true));
        }
        let insert = t.elapsed() / 100;
        println!(
            "BENCH arquivo={:.1} MB load+compactação={:?} depois={:.1} MB ({} registros) insert={:?}/registro",
            body.len() as f64 / 1e6, load, size_after as f64 / 1e6, KEEP, insert
        );
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }
}
