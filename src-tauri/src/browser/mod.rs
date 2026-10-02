//! Browser portals: a headless Chrome/Chromium driven over the DevTools
//! Protocol. Each portal node is one tab; the tile shows its screencast and
//! forwards mouse/keyboard, and agents automate it with `narrater portal …`
//! (snapshot refs, click, fill, screenshot… — the maestri-portal verbs).
//!
//! Chrome starts on first use with NarraTer's own profile (never the user's).

pub mod android;
pub mod cdp;
pub mod device;
pub mod page;

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, Weak};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, State};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::sync::oneshot;

use cdp::{Cdp, CdpEvent};
use page::{Target, UA_PRESETS};

const DEFAULT_VIEWPORT: (u32, u32) = (1280, 800);
const LAUNCH_TIMEOUT: Duration = Duration::from_secs(20);
const LOAD_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_LOG_LINES: usize = 2000;
const SCREENCAST_QUALITY: u32 = 70;

/// A portal as the canvas knows it (synced from the frontend).
#[derive(Deserialize, Clone, Debug)]
pub struct PortalEntry {
    pub id: String,
    pub label: String,
    pub url: String,
    /// Device portal: the AVD name or adb serial it shows (None: a browser tab)
    #[serde(default)]
    pub device: Option<String>,
}

pub struct Portal {
    cdp: Arc<Cdp>,
    target_id: String,
    viewport: Mutex<(u32, u32)>,
    /// Console capture; None until `logs-start`.
    logs: Mutex<Option<Vec<String>>>,
    screencast: AtomicBool,
    load_waiters: Mutex<Vec<oneshot::Sender<()>>>,
}

struct Chrome {
    child: tokio::process::Child,
    port: u16,
    cdp: Arc<Cdp>,
    /// The browser's UA without "HeadlessChrome" (sites block that).
    default_ua: String,
}

#[derive(Default)]
pub struct BrowserInner {
    chrome: Option<Chrome>,
    portals: HashMap<String, Arc<Portal>>,
    registry: HashMap<String, PortalEntry>,
    /// (portal id, terminal id): which agents may drive which portals.
    links: HashSet<(String, String)>,
}

/// Browser tabs (CDP) and device portals (adb) — both are portal nodes.
#[derive(Clone, Default)]
pub struct BrowserState(pub Arc<tokio::sync::Mutex<BrowserInner>>, pub Arc<device::Devices>);

#[derive(Serialize, Clone)]
struct FrameEvent {
    id: String,
    data: String,
    width: f64,
    height: f64,
}

#[derive(Serialize, Clone, Default)]
struct StateEvent {
    id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    loading: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

/// Chrome/Chromium to drive: $NARRATER_BROWSER, a browser on PATH, or a
/// Playwright-downloaded Chromium.
fn find_browser() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("NARRATER_BROWSER").map(PathBuf::from).filter(|p| p.exists()) {
        return Some(p);
    }
    let path = std::env::var_os("PATH").unwrap_or_default();
    for name in ["google-chrome-stable", "google-chrome", "chromium", "chromium-browser", "brave-browser", "microsoft-edge-stable"] {
        for dir in std::env::split_paths(&path) {
            let candidate = dir.join(name);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    let cache = dirs::home_dir()?.join(".cache/ms-playwright");
    let mut builds: Vec<PathBuf> = std::fs::read_dir(cache)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with("chromium-")))
        .collect();
    builds.sort();
    builds.into_iter().rev().map(|b| b.join("chrome-linux/chrome")).find(|p| p.exists())
}

async fn spawn_chrome(bin: &PathBuf, profile: &PathBuf) -> Result<(tokio::process::Child, String), String> {
    std::fs::create_dir_all(profile).map_err(|e| format!("cannot create the browser profile: {}", e))?;
    let mut command = tokio::process::Command::new(bin);
    command
        .args([
            "--headless=new",
            "--remote-debugging-port=0",
            &format!("--user-data-dir={}", profile.display()),
            "--no-first-run",
            "--no-default-browser-check",
            "--disable-background-timer-throttling",
            "--disable-renderer-backgrounding",
            &format!("--window-size={},{}", DEFAULT_VIEWPORT.0, DEFAULT_VIEWPORT.1),
            "about:blank",
        ])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);
    // Tie the browser's life to ours even on SIGKILL/crash (no orphaned
    // headless Chrome holding the profile lock). PDEATHSIG follows the
    // spawning *thread*: this runs on a tokio worker, which lives as long as
    // the runtime — i.e. the app.
    // SAFETY: prctl is async-signal-safe; nothing else runs between fork and exec.
    unsafe {
        command.pre_exec(|| {
            libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM);
            Ok(())
        });
    }
    let mut child = command
        .spawn()
        .map_err(|e| format!("cannot start {}: {}", bin.display(), e))?;
    let stderr = child.stderr.take().ok_or("no stderr")?;
    let mut lines = BufReader::new(stderr).lines();
    let ws = tokio::time::timeout(LAUNCH_TIMEOUT, async {
        while let Ok(Some(line)) = lines.next_line().await {
            if let Some(url) = line.split("DevTools listening on ").nth(1) {
                return Some(url.trim().to_string());
            }
        }
        None
    })
    .await
    .ok()
    .flatten()
    .ok_or_else(|| "the browser did not start (no DevTools endpoint)".to_string())?;
    // Keep draining stderr — a full pipe would block the browser
    tokio::spawn(async move { while let Ok(Some(_)) = lines.next_line().await {} });
    Ok((child, ws))
}

async fn launch_chrome() -> Result<Chrome, String> {
    let bin = find_browser().ok_or(
        "no Chrome/Chromium found — install Google Chrome or Chromium (or set NARRATER_BROWSER to its path) to use portals",
    )?;
    let base = dirs::data_local_dir().unwrap_or_else(std::env::temp_dir).join("narrater/browser-profile");
    // Another NarraTer may hold the profile lock: fall back to a private one
    let (child, ws) = match spawn_chrome(&bin, &base).await {
        Ok(r) => r,
        Err(_) => {
            let private = std::env::temp_dir().join(format!("narrater-{}/browser-profile", std::process::id()));
            spawn_chrome(&bin, &private).await?
        }
    };
    let port = ws
        .trim_start_matches("ws://")
        .split('/')
        .next()
        .and_then(|host| host.rsplit(':').next())
        .and_then(|p| p.parse().ok())
        .ok_or_else(|| format!("unexpected DevTools endpoint {}", ws))?;
    let (cdp, _events) = Cdp::connect(&ws).await?;
    let version = cdp.call("Browser.getVersion", json!({})).await?;
    let default_ua = version["userAgent"].as_str().unwrap_or_default().replace("HeadlessChrome", "Chrome");
    Ok(Chrome { child, port, cdp, default_ua })
}

impl BrowserInner {
    async fn chrome(&mut self) -> Result<&Chrome, String> {
        let dead = match self.chrome.as_mut() {
            Some(c) => c.child.try_wait().map(|s| s.is_some()).unwrap_or(true),
            None => true,
        };
        if dead {
            // A crashed browser took every tab with it
            self.portals.clear();
            self.chrome = Some(launch_chrome().await?);
        }
        Ok(self.chrome.as_ref().unwrap())
    }

    /// The open portal `id`, opening its tab (at the registry URL) if needed.
    async fn portal(&mut self, app: &AppHandle, id: &str, size: Option<(u32, u32)>) -> Result<Arc<Portal>, String> {
        self.portal_at(app, id, size, None).await
    }

    /// Like `portal`, with the URL to open at when the canvas registry
    /// hasn't caught up yet (a tile that mounts right after the scene loads).
    async fn portal_at(
        &mut self,
        app: &AppHandle,
        id: &str,
        size: Option<(u32, u32)>,
        url_hint: Option<&str>,
    ) -> Result<Arc<Portal>, String> {
        if let Some(p) = self.portals.get(id) {
            return Ok(Arc::clone(p));
        }
        let url = self
            .registry
            .get(id)
            .map(|e| e.url.clone())
            .or_else(|| url_hint.map(String::from))
            .unwrap_or_default();
        let chrome = self.chrome().await?;
        let (port, default_ua, browser) = (chrome.port, chrome.default_ua.clone(), Arc::clone(&chrome.cdp));
        let target = browser.call("Target.createTarget", json!({ "url": "about:blank" })).await?;
        let target_id = target["targetId"].as_str().ok_or("no target id")?.to_string();
        let (cdp, events) = Cdp::connect(&format!("ws://127.0.0.1:{}/devtools/page/{}", port, target_id)).await?;
        cdp.call("Page.enable", json!({})).await?;
        cdp.call("Runtime.enable", json!({})).await?;
        cdp.call("Emulation.setUserAgentOverride", json!({ "userAgent": default_ua })).await?;
        let viewport = size.unwrap_or(DEFAULT_VIEWPORT);
        set_viewport(&cdp, viewport).await?;

        let portal = Arc::new(Portal {
            cdp: Arc::clone(&cdp),
            target_id,
            viewport: Mutex::new(viewport),
            logs: Mutex::new(None),
            screencast: AtomicBool::new(false),
            load_waiters: Mutex::new(Vec::new()),
        });
        tokio::spawn(pump_events(app.clone(), id.to_string(), cdp, Arc::downgrade(&portal), events));
        self.portals.insert(id.to_string(), Arc::clone(&portal));
        if !url.is_empty() && url != "about:blank" {
            // Opening shouldn't fail on a dead link — the page shows the error
            let _ = navigate(&portal, &url).await;
        }
        Ok(portal)
    }

    async fn close(&mut self, id: &str) {
        if let Some(p) = self.portals.remove(id) {
            if let Some(c) = &self.chrome {
                let _ = c.cdp.call("Target.closeTarget", json!({ "targetId": p.target_id })).await;
            }
        }
    }
}

async fn set_viewport(cdp: &Cdp, (w, h): (u32, u32)) -> Result<(), String> {
    cdp.call(
        "Emulation.setDeviceMetricsOverride",
        json!({ "width": w, "height": h, "deviceScaleFactor": 1, "mobile": false }),
    )
    .await
    .map(|_| ())
}

async fn start_screencast(p: &Portal) -> Result<(), String> {
    let (w, h) = *p.viewport.lock().unwrap();
    p.cdp
        .call(
            "Page.startScreencast",
            json!({ "format": "jpeg", "quality": SCREENCAST_QUALITY, "maxWidth": w, "maxHeight": h, "everyNthFrame": 1 }),
        )
        .await
        .map(|_| ())
}

/// Forwards one tab's events: screencast frames to the tile, navigation to
/// the node (URL/title), console lines to the capture buffer.
async fn pump_events(
    app: AppHandle,
    id: String,
    cdp: Arc<Cdp>,
    portal: Weak<Portal>,
    mut events: tokio::sync::mpsc::UnboundedReceiver<CdpEvent>,
) {
    let state = |url: Option<String>, title: Option<String>, loading: Option<bool>| StateEvent {
        id: id.clone(),
        url,
        title,
        loading,
        error: None,
    };
    while let Some((method, params)) = events.recv().await {
        let Some(p) = portal.upgrade() else { break };
        match method.as_str() {
            "Page.screencastFrame" => {
                cdp.notify("Page.screencastFrameAck", json!({ "sessionId": params["sessionId"] }));
                let meta = &params["metadata"];
                let _ = app.emit("portal_frame", FrameEvent {
                    id: id.clone(),
                    data: params["data"].as_str().unwrap_or_default().to_string(),
                    width: meta["deviceWidth"].as_f64().unwrap_or(0.0),
                    height: meta["deviceHeight"].as_f64().unwrap_or(0.0),
                });
            }
            "Page.frameNavigated" if params["frame"]["parentId"].is_null() => {
                // A failed load shows chrome-error://…; keep the URL that was asked for
                let frame = &params["frame"];
                let url = frame["unreachableUrl"].as_str().or(frame["url"].as_str()).map(String::from);
                let _ = app.emit("portal_state", state(url, None, None));
            }
            "Page.navigatedWithinDocument" => {
                let url = params["url"].as_str().map(String::from);
                let _ = app.emit("portal_state", state(url, None, None));
            }
            "Page.frameStartedLoading" => {
                let _ = app.emit("portal_state", state(None, None, Some(true)));
            }
            "Page.loadEventFired" => {
                for tx in p.load_waiters.lock().unwrap().drain(..) {
                    let _ = tx.send(());
                }
                let title = eval(&p, "() => document.title", json!({})).await.ok().and_then(|v| v.as_str().map(String::from));
                let _ = app.emit("portal_state", state(None, title, Some(false)));
            }
            "Runtime.consoleAPICalled" | "Runtime.exceptionThrown" => {
                let mut logs = p.logs.lock().unwrap();
                if let Some(buf) = logs.as_mut() {
                    buf.push(console_line(&method, &params));
                    if buf.len() > MAX_LOG_LINES {
                        buf.drain(..buf.len() - MAX_LOG_LINES);
                    }
                }
            }
            "Inspector.targetCrashed" => {
                let _ = app.emit("portal_state", StateEvent { id: id.clone(), error: Some("the page crashed".into()), ..Default::default() });
            }
            _ => {}
        }
    }
}

fn console_line(method: &str, params: &Value) -> String {
    if method == "Runtime.exceptionThrown" {
        let d = &params["exceptionDetails"];
        let text = d["exception"]["description"].as_str().or(d["text"].as_str()).unwrap_or("exception");
        return format!("[exception] {}", text.lines().next().unwrap_or(text));
    }
    let args: Vec<String> = params["args"]
        .as_array()
        .map(|a| {
            a.iter()
                .map(|o| match &o["value"] {
                    Value::String(s) => s.clone(),
                    Value::Null => o["description"].as_str().unwrap_or(o["type"].as_str().unwrap_or("")).to_string(),
                    v => v.to_string(),
                })
                .collect()
        })
        .unwrap_or_default();
    format!("[{}] {}", params["type"].as_str().unwrap_or("log"), args.join(" "))
}

/// Runs a page action script (see page.rs) and returns its value.
async fn eval(p: &Portal, func: &str, args: Value) -> Result<Value, String> {
    let res = p
        .cdp
        .call(
            "Runtime.evaluate",
            json!({ "expression": page::action_expression(func, &args), "returnByValue": true, "awaitPromise": true }),
        )
        .await?;
    if let Some(ex) = res.get("exceptionDetails") {
        let text = ex["exception"]["description"].as_str().or(ex["text"].as_str()).unwrap_or("script error");
        let first = text.lines().next().unwrap_or(text);
        return Err(first.trim_start_matches("Error: ").to_string());
    }
    Ok(res["result"]["value"].clone())
}

async fn eval_str(p: &Portal, func: &str, args: Value) -> Result<String, String> {
    Ok(match eval(p, func, args).await? {
        Value::String(s) => s,
        Value::Null => String::new(),
        v => v.to_string(),
    })
}

async fn navigate(p: &Portal, url: &str) -> Result<(), String> {
    let (tx, rx) = oneshot::channel();
    p.load_waiters.lock().unwrap().push(tx);
    let res = p.cdp.call("Page.navigate", json!({ "url": url })).await?;
    if let Some(err) = res["errorText"].as_str().filter(|e| !e.is_empty()) {
        return Err(format!("{} ({})", err, url));
    }
    // Same-document navigations (#hash) never fire a load event
    if res.get("loaderId").is_some() {
        let _ = tokio::time::timeout(LOAD_TIMEOUT, rx).await;
    }
    Ok(())
}

fn normalize_url(raw: &str) -> String {
    let t = raw.trim();
    if t.is_empty() || t.contains("://") || t.starts_with("about:") || t.starts_with("data:") {
        t.to_string()
    } else if t.starts_with("localhost") || t.starts_with("127.") {
        format!("http://{}", t)
    } else {
        format!("https://{}", t)
    }
}

async fn point_of(p: &Portal, target: &str) -> Result<(f64, f64), String> {
    match page::parse_target(target) {
        Target::Point(x, y) => Ok((x, y)),
        Target::Selector(sel) => {
            let c = eval(p, page::CENTER, json!({ "sel": sel })).await?;
            Ok((c["x"].as_f64().unwrap_or(0.0), c["y"].as_f64().unwrap_or(0.0)))
        }
    }
}

async fn mouse(p: &Portal, kind: &str, (x, y): (f64, f64), button: &str, click_count: u32) -> Result<(), String> {
    p.cdp
        .call(
            "Input.dispatchMouseEvent",
            json!({ "type": kind, "x": x, "y": y, "button": button, "clickCount": click_count,
                    "buttons": if kind == "mousePressed" { 1 } else { 0 } }),
        )
        .await
        .map(|_| ())
}

async fn click_at(p: &Portal, at: (f64, f64)) -> Result<(), String> {
    mouse(p, "mouseMoved", at, "none", 0).await?;
    mouse(p, "mousePressed", at, "left", 1).await?;
    mouse(p, "mouseReleased", at, "left", 1).await
}

async fn press_key(p: &Portal, spec: &str) -> Result<(), String> {
    let k = page::parse_key(spec)?;
    for ev in page::key_events(&k) {
        p.cdp.call("Input.dispatchKeyEvent", ev).await?;
    }
    Ok(())
}

async fn insert_text(p: &Portal, text: &str) -> Result<(), String> {
    p.cdp.call("Input.insertText", json!({ "text": text })).await.map(|_| ())
}

/// Lets the page react (navigation, re-render) before the agent looks again.
async fn settle() {
    tokio::time::sleep(Duration::from_millis(150)).await;
}

fn shots_dir() -> PathBuf {
    std::env::temp_dir().join(format!("narrater-{}/screenshots", std::process::id()))
}

async fn screenshot(p: &Portal, label: &str) -> Result<String, String> {
    let shot = p.cdp.call("Page.captureScreenshot", json!({ "format": "png" })).await?;
    let data = shot["data"].as_str().ok_or("no screenshot data")?;
    use base64::Engine;
    let bytes = base64::engine::general_purpose::STANDARD.decode(data).map_err(|e| e.to_string())?;
    let dir = shots_dir();
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let safe: String = label.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '-' }).collect();
    let path = dir.join(format!("{}-{}.png", safe, crate::pty::now_ms()));
    std::fs::write(&path, bytes).map_err(|e| e.to_string())?;
    Ok(path.display().to_string())
}

fn arg<'a>(args: &'a [String], i: usize, usage: &str) -> Result<&'a str, String> {
    args.get(i).map(String::as_str).ok_or_else(|| format!("Usage: narrater portal {}", usage))
}

/// One `narrater portal <verb> "Portal" …` command.
async fn run_verb(p: &Portal, label: &str, verb: &str, args: &[String]) -> Result<String, String> {
    match verb {
        "snapshot" | "a11y" => eval_str(p, page::SNAPSHOT, json!({})).await,
        "info" => eval_str(p, page::INFO, json!({})).await,
        "screenshot" => screenshot(p, label).await,
        "navigate" => {
            let url = normalize_url(arg(args, 0, "navigate \"Portal\" \"url\"")?);
            navigate(p, &url).await?;
            eval_str(p, page::INFO, json!({})).await
        }
        "click" | "tap" => {
            let t = arg(args, 0, "click \"Portal\" @e3|css|x,y")?;
            click_at(p, point_of(p, t).await?).await?;
            settle().await;
            Ok(format!("ok: clicked {}", t))
        }
        "hover" => {
            let t = arg(args, 0, "hover \"Portal\" @e3")?;
            mouse(p, "mouseMoved", point_of(p, t).await?, "none", 0).await?;
            Ok(format!("ok: hovering {}", t))
        }
        "focus" => {
            let t = arg(args, 0, "focus \"Portal\" @e2")?;
            eval(p, page::FOCUS, json!({ "sel": t })).await?;
            Ok(format!("ok: focused {}", t))
        }
        "fill" => {
            let t = arg(args, 0, "fill \"Portal\" @e2 \"text\"")?;
            let text = arg(args, 1, "fill \"Portal\" @e2 \"text\"")?;
            eval(p, page::CLEAR, json!({ "sel": t })).await?;
            if !text.is_empty() {
                insert_text(p, text).await?;
            }
            Ok(format!("ok: filled {}", t))
        }
        "type" => {
            // type "Portal" [@target] "text"
            let (target, text) = match args {
                [t, text] => (Some(t.as_str()), text.as_str()),
                [text] => (None, text.as_str()),
                _ => return Err("Usage: narrater portal type \"Portal\" [@e2] \"text\"".into()),
            };
            if let Some(t) = target {
                eval(p, page::FOCUS, json!({ "sel": t })).await?;
            }
            insert_text(p, text).await?;
            Ok("ok: typed".into())
        }
        "key" => {
            let k = arg(args, 0, "key \"Portal\" \"Enter\"")?;
            press_key(p, k).await?;
            settle().await;
            Ok(format!("ok: pressed {}", k))
        }
        "selectall" => {
            eval(p, page::SELECT_ALL, json!({ "sel": args.first() })).await?;
            Ok("ok: selected all".into())
        }
        "clear" => {
            eval(p, page::CLEAR, json!({ "sel": args.first() })).await?;
            Ok("ok: cleared".into())
        }
        "select" => {
            let t = arg(args, 0, "select \"Portal\" @e5 \"Option\"")?;
            let opt = arg(args, 1, "select \"Portal\" @e5 \"Option\"")?;
            let picked = eval_str(p, page::SELECT_OPTION, json!({ "sel": t, "option": opt })).await?;
            Ok(format!("ok: selected \"{}\"", picked))
        }
        "check" | "uncheck" => {
            // `check "Portal"` with no target is a peek: a screenshot path
            let Some(t) = args.first() else {
                return screenshot(p, label).await;
            };
            let want = verb == "check";
            let is = eval(p, page::IS_CHECKED, json!({ "sel": t })).await?.as_bool().unwrap_or(false);
            if is != want {
                click_at(p, point_of(p, t).await?).await?;
            }
            Ok(format!("ok: {} {}", if want { "checked" } else { "unchecked" }, t))
        }
        "scroll" => {
            let dir = arg(args, 0, "scroll \"Portal\" down 300 [@e5|x,y]")?;
            let amount: f64 = args.get(1).and_then(|a| a.parse().ok()).unwrap_or(300.0);
            let (dx, dy) = match dir {
                "down" => (0.0, amount),
                "up" => (0.0, -amount),
                "right" => (amount, 0.0),
                "left" => (-amount, 0.0),
                _ => return Err("scroll direction must be up, down, left or right".into()),
            };
            let at = match args.get(2) {
                Some(t) => point_of(p, t).await?,
                None => {
                    let (w, h) = *p.viewport.lock().unwrap();
                    (w as f64 / 2.0, h as f64 / 2.0)
                }
            };
            p.cdp
                .call("Input.dispatchMouseEvent", json!({ "type": "mouseWheel", "x": at.0, "y": at.1, "deltaX": dx, "deltaY": dy }))
                .await?;
            settle().await;
            Ok(format!("ok: scrolled {} {}", dir, amount))
        }
        "scrollintoview" => {
            let t = arg(args, 0, "scrollintoview \"Portal\" @e10")?;
            eval(p, page::SCROLL_INTO_VIEW, json!({ "sel": t })).await?;
            Ok(format!("ok: {} is in view", t))
        }
        "drag" => {
            let from = point_of(p, arg(args, 0, "drag \"Portal\" @e3 @e7")?).await?;
            let to = point_of(p, arg(args, 1, "drag \"Portal\" @e3 @e7")?).await?;
            mouse(p, "mouseMoved", from, "none", 0).await?;
            mouse(p, "mousePressed", from, "left", 1).await?;
            for i in 1..=10 {
                let f = i as f64 / 10.0;
                let at = (from.0 + (to.0 - from.0) * f, from.1 + (to.1 - from.1) * f);
                p.cdp.call("Input.dispatchMouseEvent", json!({ "type": "mouseMoved", "x": at.0, "y": at.1, "button": "left", "buttons": 1 })).await?;
            }
            mouse(p, "mouseReleased", to, "left", 1).await?;
            Ok("ok: dragged".into())
        }
        "text" => {
            let t = arg(args, 0, "text \"Portal\" @e1")?;
            eval_str(p, page::TEXT, json!({ "sel": t })).await
        }
        "html" => eval_str(p, page::HTML, json!({})).await,
        "evaluate" => {
            let js = arg(args, 0, "evaluate \"Portal\" \"js\"")?;
            let res = p
                .cdp
                .call("Runtime.evaluate", json!({ "expression": js, "returnByValue": true, "awaitPromise": true }))
                .await?;
            if let Some(ex) = res.get("exceptionDetails") {
                let text = ex["exception"]["description"].as_str().or(ex["text"].as_str()).unwrap_or("script error");
                return Err(text.to_string());
            }
            Ok(match &res["result"]["value"] {
                Value::String(s) => s.clone(),
                Value::Null => res["result"]["description"].as_str().unwrap_or("undefined").to_string(),
                v => serde_json::to_string_pretty(v).unwrap_or_default(),
            })
        }
        "wait" => {
            let t = arg(args, 0, "wait \"Portal\" @e3 [timeoutMs]")?;
            let ms: u64 = args.get(1).and_then(|a| a.parse().ok()).unwrap_or(5000);
            let until = Instant::now() + Duration::from_millis(ms);
            loop {
                if eval(p, page::EXISTS, json!({ "sel": t })).await?.as_bool() == Some(true) {
                    return Ok(format!("ok: {} is there", t));
                }
                if Instant::now() >= until {
                    return Err(format!("{} did not appear within {}ms", t, ms));
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        }
        "logs-start" => {
            *p.logs.lock().unwrap() = Some(Vec::new());
            Ok("ok: capturing console output — read it with `narrater portal logs`".into())
        }
        "logs" => {
            let mut logs = p.logs.lock().unwrap();
            match logs.as_mut() {
                None => Err("console capture is off — run `narrater portal logs-start` first".into()),
                Some(buf) if buf.is_empty() => Ok("(no console output)".into()),
                Some(buf) => Ok(std::mem::take(buf).join("\n")),
            }
        }
        "resize" => {
            let w: u32 = arg(args, 0, "resize \"Portal\" 390 844")?.parse().map_err(|_| "width must be a number")?;
            let h: u32 = arg(args, 1, "resize \"Portal\" 390 844")?.parse().map_err(|_| "height must be a number")?;
            if !(100..=4000).contains(&w) || !(100..=4000).contains(&h) {
                return Err("viewport must be between 100 and 4000 px".into());
            }
            set_viewport(&p.cdp, (w, h)).await?;
            *p.viewport.lock().unwrap() = (w, h);
            if p.screencast.load(Ordering::Relaxed) {
                let _ = p.cdp.call("Page.stopScreencast", json!({})).await;
                start_screencast(p).await?;
            }
            eval_str(p, page::VIEWPORT, json!({})).await
        }
        _ => Err(format!("unknown portal command '{}' — run `narrater help portal`", verb)),
    }
}

impl BrowserState {
    /// `narrater portal <verb> "Portal" …` from terminal `from`. The portal
    /// must be connected to that terminal on the canvas.
    pub async fn run_cli(&self, app: &AppHandle, from: &str, name: &str, verb: &str, args: &[String]) -> String {
        if verb == "devices" {
            return self.1.list_text().await;
        }
        let mut inner = self.0.lock().await;
        let id = match resolve_portal(&inner, from, name) {
            Ok(id) => id,
            Err(e) => return e,
        };
        let label = inner.registry.get(&id).map(|e| e.label.clone()).unwrap_or_default();
        if let Some(dev) = inner.registry.get(&id).and_then(|e| e.device.clone()) {
            // Device portal: the browser lock isn't needed (a boot can take minutes)
            drop(inner);
            let result = match self.1.portal(&id, &dev).await {
                Ok(p) => device::run_verb(&p, &label, verb, args).await,
                Err(e) => Err(e),
            };
            return result.unwrap_or_else(|e| format!("Error: {}", e));
        }
        if verb == "ua" {
            return match ua(&mut inner, app, &id, args).await {
                Ok(s) => s,
                Err(e) => format!("Error: {}", e),
            };
        }
        let portal = match inner.portal(app, &id, None).await {
            Ok(p) => p,
            Err(e) => return format!("Error: {}", e),
        };
        // Long verbs (navigate, wait) must not block the other portals
        drop(inner);
        match run_verb(&portal, &label, verb, args).await {
            Ok(s) => s,
            Err(e) => format!("Error: {}", e),
        }
    }
}

async fn ua(inner: &mut BrowserInner, app: &AppHandle, id: &str, args: &[String]) -> Result<String, String> {
    let p = inner.portal(app, id, None).await?;
    let Some(name) = args.first() else {
        let current = eval_str(&p, "() => navigator.userAgent", json!({})).await?;
        return Ok(format!("user agent: {}\npresets: {}", current, UA_PRESETS));
    };
    let ua = match page::ua_preset(name) {
        Some(Some(ua)) => ua.to_string(),
        Some(None) => inner.chrome().await?.default_ua.clone(),
        None => return Err(format!("unknown preset '{}' — presets: {}", name, UA_PRESETS)),
    };
    p.cdp.call("Emulation.setUserAgentOverride", json!({ "userAgent": ua })).await?;
    let (tx, rx) = oneshot::channel();
    p.load_waiters.lock().unwrap().push(tx);
    p.cdp.call("Page.reload", json!({})).await?;
    let _ = tokio::time::timeout(LOAD_TIMEOUT, rx).await;
    Ok(format!("ok: user agent '{}' — page reloaded", name))
}

fn resolve_portal(inner: &BrowserInner, from: &str, name: &str) -> Result<String, String> {
    let wanted = name.trim();
    let mine: Vec<&PortalEntry> = inner
        .registry
        .values()
        .filter(|e| inner.links.contains(&(e.id.clone(), from.to_string())))
        .collect();
    let exact: Vec<&&PortalEntry> = mine.iter().filter(|e| e.label == wanted).collect();
    let hits = if exact.is_empty() {
        mine.iter().filter(|e| e.label.to_lowercase() == wanted.to_lowercase()).collect()
    } else {
        exact
    };
    match hits.as_slice() {
        [one] => Ok(one.id.clone()),
        [] => Err(format!(
            "Error: no connected portal named '{}' — run `narrater list`, or `narrater portal create URL \"Name\"`",
            wanted
        )),
        _ => Err(format!("Error: '{}' matches several portals — rename one on the canvas", wanted)),
    }
}

// ---- Tauri commands (the tile) -------------------------------------------

/// Mirrors the canvas portals and their terminal links; closes the tabs of
/// portals that were removed.
#[tauri::command]
pub async fn portals_sync(
    portals: Vec<PortalEntry>,
    links: Vec<(String, String)>,
    state: State<'_, BrowserState>,
) -> Result<(), String> {
    let mut inner = state.0.lock().await;
    inner.registry = portals.into_iter().map(|p| (p.id.clone(), p)).collect();
    inner.links = links.into_iter().collect();
    let gone: Vec<String> = inner.portals.keys().filter(|id| !inner.registry.contains_key(*id)).cloned().collect();
    for id in gone {
        inner.close(&id).await;
    }
    let device_nodes: HashSet<String> =
        inner.registry.values().filter(|e| e.device.is_some()).map(|e| e.id.clone()).collect();
    drop(inner);
    state.1.retain(&device_nodes).await;
    Ok(())
}

/// Opens a portal's tab now (created by an agent with --size).
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn portal_open(
    id: String,
    url: Option<String>,
    width: Option<u32>,
    height: Option<u32>,
    device: Option<String>,
    app: AppHandle,
    state: State<'_, BrowserState>,
) -> Result<(), String> {
    if let Some(dev) = device {
        return state.1.portal(&id, &dev).await.map(|_| ());
    }
    let size = width.zip(height);
    state.0.lock().await.portal_at(&app, &id, size, url.as_deref()).await.map(|_| ())
}

/// The tile is on screen: open the tab if needed and stream it.
#[tauri::command]
pub async fn portal_attach(
    id: String,
    url: Option<String>,
    device: Option<String>,
    app: AppHandle,
    state: State<'_, BrowserState>,
) -> Result<(), String> {
    if let Some(dev) = device {
        let p = state.1.portal(&id, &dev).await?;
        device::start_stream(app, id, p);
        return Ok(());
    }
    let portal = state.0.lock().await.portal_at(&app, &id, None, url.as_deref()).await?;
    portal.screencast.store(true, Ordering::Relaxed);
    start_screencast(&portal).await
}

#[tauri::command]
pub async fn portal_detach(id: String, state: State<'_, BrowserState>) -> Result<(), String> {
    if let Some(p) = state.1.get(&id) {
        device::stop_stream(&p);
        return Ok(());
    }
    let portal = state.0.lock().await.portals.get(&id).cloned();
    if let Some(p) = portal {
        p.screencast.store(false, Ordering::Relaxed);
        let _ = p.cdp.call("Page.stopScreencast", json!({})).await;
    }
    Ok(())
}

async fn open_portal(state: &State<'_, BrowserState>, app: &AppHandle, id: &str) -> Result<Arc<Portal>, String> {
    state.0.lock().await.portal(app, id, None).await
}

#[tauri::command]
pub async fn portal_navigate(id: String, url: String, app: AppHandle, state: State<'_, BrowserState>) -> Result<(), String> {
    let p = open_portal(&state, &app, &id).await?;
    navigate(&p, &normalize_url(&url)).await
}

/// Back (-1), forward (1) or reload (0).
#[tauri::command]
pub async fn portal_history(id: String, delta: i32, app: AppHandle, state: State<'_, BrowserState>) -> Result<(), String> {
    let p = open_portal(&state, &app, &id).await?;
    if delta == 0 {
        p.cdp.call("Page.reload", json!({})).await.map(|_| ())
    } else {
        p.cdp.call("Runtime.evaluate", json!({ "expression": format!("history.go({})", delta) })).await.map(|_| ())
    }
}

/// Raw input from the tile (already in page CSS pixels). Fire-and-forget:
/// mouse moves come fast and must not queue behind each other.
#[tauri::command]
pub async fn portal_input(id: String, method: String, params: Value, state: State<'_, BrowserState>) -> Result<(), String> {
    if !matches!(method.as_str(), "Input.dispatchMouseEvent" | "Input.dispatchKeyEvent" | "Input.insertText") {
        return Err("unsupported input".into());
    }
    let portal = state.0.lock().await.portals.get(&id).cloned();
    if let Some(p) = portal {
        p.cdp.notify(&method, params);
    }
    Ok(())
}

/// Taps, swipes, keys and text from a device portal's tile (device pixels).
#[tauri::command]
pub async fn device_input(id: String, kind: String, params: Value, state: State<'_, BrowserState>) -> Result<(), String> {
    let p = state.1.get(&id).ok_or("the device isn't connected yet")?;
    device::input(&p, &kind, &params).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urls_get_a_scheme() {
        assert_eq!(normalize_url("example.com"), "https://example.com");
        assert_eq!(normalize_url("localhost:3000"), "http://localhost:3000");
        assert_eq!(normalize_url("http://x.dev/a"), "http://x.dev/a");
        assert_eq!(normalize_url("about:blank"), "about:blank");
    }

    #[test]
    fn console_lines_are_readable() {
        let p = json!({ "type": "warn", "args": [{ "type": "string", "value": "low" }, { "type": "number", "value": 3 }] });
        assert_eq!(console_line("Runtime.consoleAPICalled", &p), "[warn] low 3");
        let e = json!({ "exceptionDetails": { "exception": { "description": "TypeError: x is undefined\n    at a.js:1" } } });
        assert_eq!(console_line("Runtime.exceptionThrown", &e), "[exception] TypeError: x is undefined");
    }

    #[test]
    fn portals_resolve_only_when_connected() {
        let mut inner = BrowserInner::default();
        inner.registry.insert("p1".into(), PortalEntry { id: "p1".into(), label: "Docs".into(), url: String::new(), device: None });
        inner.registry.insert("p2".into(), PortalEntry { id: "p2".into(), label: "Admin".into(), url: String::new(), device: None });
        inner.links.insert(("p1".into(), "t1".into()));
        assert_eq!(resolve_portal(&inner, "t1", "docs").unwrap(), "p1");
        assert!(resolve_portal(&inner, "t1", "Admin").unwrap_err().contains("no connected portal"));
        assert!(resolve_portal(&inner, "t2", "Docs").is_err());
    }
}
