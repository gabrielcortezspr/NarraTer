//! Device portals: an Android emulator or phone on the canvas, driven over
//! adb (Maestri's maestri-portal-devices). The tile shows the device screen
//! (screencap polling) and forwards taps, swipes, keys and text; agents use
//! the same `narrater portal …` verbs as for browsers, minus browser-only ones.
//!
//! Emulators are Android Studio's AVDs: NarraTer boots one headless when a
//! portal asks for it (never changing its settings) and stops the ones it
//! booted when no portal uses them anymore.

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::{AppHandle, Emitter};

use super::android::{self, Bounds};
use super::page::{parse_target, Target};

const ADB_TIMEOUT: Duration = Duration::from_secs(30);
const BOOT_TIMEOUT: Duration = Duration::from_secs(240);
const FRAME_INTERVAL: Duration = Duration::from_millis(250);
const FRAME_MAX_HEIGHT: u32 = 960;
const FRAME_QUALITY: u8 = 70;

pub struct Sdk {
    adb: PathBuf,
    emulator: Option<PathBuf>,
}

/// $ANDROID_HOME, $ANDROID_SDK_ROOT, ~/Android/Sdk — or adb on PATH.
fn sdk() -> Option<Sdk> {
    let roots = ["ANDROID_HOME", "ANDROID_SDK_ROOT"]
        .iter()
        .filter_map(|v| std::env::var_os(v).map(PathBuf::from))
        .chain(dirs::home_dir().map(|h| h.join("Android/Sdk")));
    for root in roots {
        let adb = root.join("platform-tools/adb");
        if adb.is_file() {
            let emulator = Some(root.join("emulator/emulator")).filter(|e| e.is_file());
            return Some(Sdk { adb, emulator });
        }
    }
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|d| d.join("adb"))
        .find(|p| p.is_file())
        .map(|adb| Sdk { adb, emulator: None })
}

fn require_sdk() -> Result<Sdk, String> {
    sdk().ok_or_else(|| {
        "no Android SDK found — install Android Studio (or platform-tools), or set ANDROID_HOME".to_string()
    })
}

async fn adb(sdk: &Sdk, serial: Option<&str>, args: &[&str]) -> Result<Vec<u8>, String> {
    let mut cmd = tokio::process::Command::new(&sdk.adb);
    if let Some(s) = serial {
        cmd.args(["-s", s]);
    }
    cmd.args(args).kill_on_drop(true);
    let out = tokio::time::timeout(ADB_TIMEOUT, cmd.output())
        .await
        .map_err(|_| format!("adb {} timed out", args.first().unwrap_or(&"")))?
        .map_err(|e| format!("cannot run adb: {}", e))?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        return Err(format!("adb {}: {}", args.join(" "), err.trim()));
    }
    Ok(out.stdout)
}

async fn adb_text(sdk: &Sdk, serial: Option<&str>, args: &[&str]) -> Result<String, String> {
    adb(sdk, serial, args).await.map(|b| String::from_utf8_lossy(&b).trim().to_string())
}

/// AVD name of a running emulator.
async fn avd_of(sdk: &Sdk, serial: &str) -> Option<String> {
    adb_text(sdk, Some(serial), &["emu", "avd", "name"])
        .await
        .ok()
        .and_then(|o| o.lines().next().map(|l| l.trim().to_string()))
        .filter(|n| !n.is_empty() && n != "OK")
}

async fn list_avds(sdk: &Sdk) -> Vec<String> {
    let Some(emulator) = &sdk.emulator else { return Vec::new() };
    tokio::process::Command::new(emulator)
        .arg("-list-avds")
        .output()
        .await
        .map(|o| {
            String::from_utf8_lossy(&o.stdout)
                .lines()
                .map(str::trim)
                .filter(|l| !l.is_empty() && !l.starts_with("INFO"))
                .map(String::from)
                .collect()
        })
        .unwrap_or_default()
}

struct Booted {
    child: tokio::process::Child,
    serial: Option<String>,
}

pub struct DevicePortal {
    pub serial: String,
    refs: Mutex<Vec<Bounds>>,
    streaming: AtomicBool,
    screen: Mutex<(u32, u32)>,
}

#[derive(Default)]
pub struct Devices {
    /// Device portals by canvas node id.
    portals: Mutex<HashMap<String, Arc<DevicePortal>>>,
    /// Emulators NarraTer booted, by AVD name (one boot at a time).
    booted: tokio::sync::Mutex<HashMap<String, Booted>>,
}

#[derive(Serialize, Clone)]
struct FrameEvent {
    id: String,
    data: String,
    width: f64,
    height: f64,
}

fn state_hint(state: &str) -> String {
    match state {
        "unauthorized" => "unauthorized — unlock the phone and accept the USB debugging prompt".into(),
        "offline" => "offline — reconnect it, or restart adb".into(),
        s => s.to_string(),
    }
}

impl Devices {
    /// `narrater portal devices`.
    pub async fn list_text(&self) -> String {
        let Ok(sdk) = require_sdk() else {
            return "Error: no Android SDK found — install Android Studio (or platform-tools), or set ANDROID_HOME".into();
        };
        let devices = adb_text(&sdk, None, &["devices", "-l"]).await.map(|o| android::parse_devices(&o)).unwrap_or_default();
        let used: Vec<String> = self.portals.lock().unwrap().values().map(|p| p.serial.clone()).collect();
        let mut out = vec!["Devices:".to_string()];
        let mut running_avds = HashMap::new();
        for d in &devices {
            let avd = if d.is_emulator() { avd_of(&sdk, &d.serial).await } else { None };
            if let Some(a) = &avd {
                running_avds.insert(a.clone(), d.serial.clone());
            }
            let what = match (&avd, d.is_emulator()) {
                (Some(a), _) => format!("{} (emulator)", a),
                (None, true) => "emulator".into(),
                (None, false) => format!("{} (phone)", d.model.clone().unwrap_or_else(|| "device".into())),
            };
            let status = if d.state == "device" { "online".into() } else { state_hint(&d.state) };
            let n = used.iter().filter(|s| **s == d.serial).count();
            let usage = if n == 0 { "free".to_string() } else { format!("in {} portal{}", n, if n == 1 { "" } else { "s" }) };
            out.push(format!("  {}  {}  {}  {}", d.serial, what, status, usage));
        }
        if devices.is_empty() {
            out.push("  (none — plug in a phone with USB debugging, or boot an AVD below)".into());
        }
        let avds = list_avds(&sdk).await;
        out.push(String::new());
        out.push("AVDs (Android Studio):".into());
        if avds.is_empty() {
            out.push("  (none — create one in Android Studio's Device Manager)".into());
        }
        for a in avds {
            match running_avds.get(&a) {
                Some(serial) => out.push(format!("  {}  running as {}", a, serial)),
                None => out.push(format!("  {}  not running — `narrater portal create --simulator {}` boots it", a, a)),
            }
        }
        out.join("\n")
    }

    /// Serial for `id` (a serial or an AVD name), booting the AVD if needed.
    async fn resolve(&self, id: &str) -> Result<String, String> {
        let sdk = require_sdk()?;
        let devices = adb_text(&sdk, None, &["devices", "-l"]).await.map(|o| android::parse_devices(&o))?;
        if let Some(d) = devices.iter().find(|d| d.serial == id) {
            return if d.state == "device" { Ok(d.serial.clone()) } else { Err(format!("'{}' is {}", id, state_hint(&d.state))) };
        }
        for d in devices.iter().filter(|d| d.is_emulator() && d.state == "device") {
            if avd_of(&sdk, &d.serial).await.as_deref() == Some(id) {
                return Ok(d.serial.clone());
            }
        }
        if !list_avds(&sdk).await.iter().any(|a| a == id) {
            return Err(format!("no device or AVD named '{}' — run `narrater portal devices`", id));
        }
        self.boot(&sdk, id).await
    }

    async fn boot(&self, sdk: &Sdk, avd: &str) -> Result<String, String> {
        let mut booted = self.booted.lock().await;
        let emulator = sdk.emulator.as_ref().ok_or("the Android SDK has no emulator installed")?;
        if !booted.contains_key(avd) {
            let mut cmd = tokio::process::Command::new(emulator);
            cmd.args(["-avd", avd, "-no-window", "-no-audio", "-no-boot-anim"]);
            // Extra flags for special setups (e.g. -read-only, -gpu swiftshader_indirect)
            if let Ok(extra) = std::env::var("NARRATER_EMULATOR_ARGS") {
                cmd.args(extra.split_whitespace());
            }
            cmd.stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .kill_on_drop(true);
            // SAFETY: prctl is async-signal-safe; nothing else runs between fork and exec.
            unsafe {
                cmd.pre_exec(|| {
                    libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM);
                    Ok(())
                });
            }
            let child = cmd.spawn().map_err(|e| format!("cannot start the emulator: {}", e))?;
            booted.insert(avd.to_string(), Booted { child, serial: None });
        }
        let started = Instant::now();
        loop {
            if let Some(b) = booted.get_mut(avd) {
                if let Ok(Some(status)) = b.child.try_wait() {
                    booted.remove(avd);
                    return Err(format!("the emulator for '{}' exited ({}) — open it once in Android Studio to check it boots", avd, status));
                }
            }
            if let Ok(out) = adb_text(sdk, None, &["devices", "-l"]).await {
                for d in android::parse_devices(&out).into_iter().filter(|d| d.is_emulator() && d.state == "device") {
                    if avd_of(sdk, &d.serial).await.as_deref() != Some(avd) {
                        continue;
                    }
                    let done = adb_text(sdk, Some(&d.serial), &["shell", "getprop", "sys.boot_completed"]).await;
                    if done.as_deref() == Ok("1") {
                        if let Some(b) = booted.get_mut(avd) {
                            b.serial = Some(d.serial.clone());
                        }
                        return Ok(d.serial);
                    }
                }
            }
            if started.elapsed() > BOOT_TIMEOUT {
                return Err(format!("'{}' did not finish booting in {}s", avd, BOOT_TIMEOUT.as_secs()));
            }
            tokio::time::sleep(Duration::from_secs(2)).await;
        }
    }

    /// The device portal of node `node`, opening it on device `device_id`.
    pub async fn portal(&self, node: &str, device_id: &str) -> Result<Arc<DevicePortal>, String> {
        if let Some(p) = self.portals.lock().unwrap().get(node) {
            return Ok(Arc::clone(p));
        }
        let serial = self.resolve(device_id).await?;
        let sdk = require_sdk()?;
        let size = adb_text(&sdk, Some(&serial), &["shell", "wm", "size"])
            .await
            .ok()
            .and_then(|o| android::parse_wm_size(&o))
            .unwrap_or((1080, 1920));
        let portal = Arc::new(DevicePortal {
            serial,
            refs: Mutex::new(Vec::new()),
            streaming: AtomicBool::new(false),
            screen: Mutex::new(size),
        });
        self.portals.lock().unwrap().entry(node.to_string()).or_insert_with(|| Arc::clone(&portal));
        Ok(portal)
    }

    pub fn get(&self, node: &str) -> Option<Arc<DevicePortal>> {
        self.portals.lock().unwrap().get(node).cloned()
    }

    /// Drops device portals whose nodes are gone, and stops emulators
    /// NarraTer booted that no portal uses anymore.
    pub async fn retain(&self, alive: &std::collections::HashSet<String>) {
        let in_use: Vec<String> = {
            let mut portals = self.portals.lock().unwrap();
            portals.retain(|node, p| {
                let keep = alive.contains(node);
                if !keep {
                    p.streaming.store(false, Ordering::Relaxed);
                }
                keep
            });
            portals.values().map(|p| p.serial.clone()).collect()
        };
        let mut booted = self.booted.lock().await;
        let idle: Vec<String> = booted
            .iter()
            .filter(|(_, b)| b.serial.as_ref().is_some_and(|s| !in_use.contains(s)))
            .map(|(avd, _)| avd.clone())
            .collect();
        for avd in idle {
            if let Some(mut b) = booted.remove(&avd) {
                // Graceful: lets the emulator save its state
                if let (Ok(sdk), Some(serial)) = (require_sdk(), b.serial.as_deref()) {
                    let _ = adb(&sdk, Some(serial), &["emu", "kill"]).await;
                }
                let _ = tokio::time::timeout(Duration::from_secs(10), b.child.wait()).await;
                let _ = b.child.start_kill();
            }
        }
    }
}

impl DevicePortal {
    async fn shell(&self, args: &[&str]) -> Result<String, String> {
        let sdk = require_sdk()?;
        let mut full = vec!["shell"];
        full.extend_from_slice(args);
        adb_text(&sdk, Some(&self.serial), &full).await
    }

    fn screen(&self) -> (u32, u32) {
        *self.screen.lock().unwrap()
    }

    fn point(&self, target: &str) -> Result<(i32, i32), String> {
        match parse_target(target) {
            Target::Point(x, y) => Ok((x as i32, y as i32)),
            Target::Selector(sel) => {
                let Some(n) = sel.strip_prefix("@e").and_then(|n| n.parse::<usize>().ok()) else {
                    return Err(format!("on a device, targets are @refs from snapshot or x,y coordinates (got '{}')", sel));
                };
                self.refs
                    .lock()
                    .unwrap()
                    .get(n.wrapping_sub(1))
                    .map(Bounds::center)
                    .ok_or_else(|| format!("no element {} — refs change with the screen; run snapshot again", sel))
            }
        }
    }

    async fn tap(&self, (x, y): (i32, i32)) -> Result<(), String> {
        self.shell(&["input", "tap", &x.to_string(), &y.to_string()]).await.map(|_| ())
    }

    async fn swipe(&self, c: [i32; 4], ms: u32) -> Result<(), String> {
        let a: Vec<String> = c.iter().map(i32::to_string).chain([ms.to_string()]).collect();
        let mut args = vec!["input", "swipe"];
        args.extend(a.iter().map(String::as_str));
        self.shell(&args).await.map(|_| ())
    }

    async fn keyevent(&self, code: u32) -> Result<(), String> {
        self.shell(&["input", "keyevent", &code.to_string()]).await.map(|_| ())
    }

    async fn type_text(&self, text: &str) -> Result<(), String> {
        for (i, line) in text.split('\n').enumerate() {
            if i > 0 {
                self.keyevent(66).await?;
            }
            if !line.is_empty() {
                let arg = android::input_text_arg(line)?;
                self.shell(&["input", "text", &arg]).await?;
            }
        }
        Ok(())
    }

    async fn screencap(&self) -> Result<Vec<u8>, String> {
        let sdk = require_sdk()?;
        let png = adb(&sdk, Some(&self.serial), &["exec-out", "screencap", "-p"]).await?;
        if !png.starts_with(b"\x89PNG") {
            return Err("the device returned no screenshot (is it locked or still booting?)".into());
        }
        Ok(png)
    }

    async fn snapshot(&self) -> Result<String, String> {
        let sdk = require_sdk()?;
        let raw = adb(&sdk, Some(&self.serial), &["exec-out", "uiautomator", "dump", "/dev/tty"]).await?;
        let xml = String::from_utf8_lossy(&raw);
        let (text, refs) = android::snapshot(&xml, self.screen())?;
        *self.refs.lock().unwrap() = refs;
        Ok(text)
    }

    async fn info(&self) -> Result<String, String> {
        let prop = |k: &'static str| async move { self.shell(&["getprop", k]).await.unwrap_or_default() };
        let (model, release, api) = (prop("ro.product.model").await, prop("ro.build.version.release").await, prop("ro.build.version.sdk").await);
        let sdk = require_sdk()?;
        let avd = if self.serial.starts_with("emulator-") { avd_of(&sdk, &self.serial).await } else { None };
        let focus = self.shell(&["dumpsys", "window"]).await.unwrap_or_default();
        let app = focus
            .lines()
            .find(|l| l.contains("mCurrentFocus"))
            .and_then(|l| l.split_whitespace().last())
            .map(|s| s.trim_end_matches('}').to_string())
            .unwrap_or_else(|| "?".into());
        let (w, h) = self.screen();
        Ok(format!(
            "device: {}{}\nserial: {}\nandroid: {} (API {})\nstate: online\ndisplay: {}x{} px\nforeground: {}",
            model,
            avd.map(|a| format!(" (emulator {})", a)).unwrap_or_default(),
            self.serial,
            release,
            api,
            w,
            h,
            app
        ))
    }
}

fn shots_dir() -> PathBuf {
    std::env::temp_dir().join(format!("narrater-{}/screenshots", std::process::id()))
}

fn arg<'a>(args: &'a [String], i: usize, usage: &str) -> Result<&'a str, String> {
    args.get(i).map(String::as_str).ok_or_else(|| format!("Usage: narrater portal {}", usage))
}

/// One `narrater portal <verb> "Device" …` on a device portal.
pub async fn run_verb(p: &DevicePortal, label: &str, verb: &str, args: &[String]) -> Result<String, String> {
    match verb {
        "snapshot" | "a11y" => p.snapshot().await,
        "info" => p.info().await,
        "screenshot" | "check" if args.is_empty() || verb == "screenshot" => {
            let png = p.screencap().await?;
            let dir = shots_dir();
            std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
            let safe: String = label.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '-' }).collect();
            let path = dir.join(format!("{}-{}.png", safe, crate::pty::now_ms()));
            std::fs::write(&path, png).map_err(|e| e.to_string())?;
            Ok(path.display().to_string())
        }
        "click" | "tap" => {
            let t = arg(args, 0, "click \"Device\" @e3|x,y")?;
            p.tap(p.point(t)?).await?;
            Ok(format!("ok: tapped {}", t))
        }
        "type" => {
            let (target, text) = match args {
                [t, text] => (Some(t.as_str()), text.as_str()),
                [text] => (None, text.as_str()),
                _ => return Err("Usage: narrater portal type \"Device\" [@e2] \"text\"".into()),
            };
            if let Some(t) = target {
                p.tap(p.point(t)?).await?;
            }
            p.type_text(text).await?;
            Ok("ok: typed".into())
        }
        "key" => {
            let k = arg(args, 0, "key \"Device\" Enter|back")?;
            match android::keycode(k) {
                Some(code) => p.keyevent(code).await?,
                None if k.chars().count() == 1 => p.type_text(k).await?,
                None => return Err(format!("unknown key '{}' — Enter, Tab, Escape, Backspace, Delete, Space, Arrow*, back, home, recents, menu, power, volumeup, volumedown", k)),
            }
            Ok(format!("ok: pressed {}", k))
        }
        "button" | "home" | "lock" => {
            let name = match verb {
                "home" => "home",
                "lock" => "power",
                _ => arg(args, 0, "button \"Device\" home|back|recents|menu|power|volumeup|volumedown")?,
            };
            let code = android::keycode(name).ok_or_else(|| format!("unknown button '{}' — back, home, recents, menu, power, volumeup, volumedown", name))?;
            p.keyevent(code).await?;
            Ok(format!("ok: pressed {}", name))
        }
        "scroll" => {
            let dir = arg(args, 0, "scroll \"Device\" down 400 [@e5|x,y]")?;
            let amount: i32 = args.get(1).and_then(|a| a.parse().ok()).unwrap_or(600);
            let (w, h) = p.screen();
            let at = match args.get(2) {
                Some(t) => p.point(t)?,
                None => (w as i32 / 2, h as i32 / 2),
            };
            p.swipe(android::scroll_swipe(dir, amount, at, (w, h))?, 300).await?;
            Ok(format!("ok: scrolled {} {}", dir, amount))
        }
        "swipe" | "drag" => {
            let a = p.point(arg(args, 0, "swipe \"Device\" @e3 @e7")?)?;
            let b = p.point(arg(args, 1, "swipe \"Device\" @e3 @e7")?)?;
            p.swipe([a.0, a.1, b.0, b.1], if verb == "drag" { 900 } else { 300 }).await?;
            Ok(format!("ok: {}d", verb.trim_end_matches('e')))
        }
        "launch" => {
            let pkg = arg(args, 0, "launch \"Device\" com.example.app")?;
            let not_found = || format!("'{}' is not installed or has no launcher activity", pkg);
            let out = p.shell(&["monkey", "-p", pkg, "-c", "android.intent.category.LAUNCHER", "1"]).await.map_err(|_| not_found())?;
            if out.contains("No activities found") || out.contains("monkey aborted") {
                return Err(not_found());
            }
            Ok(format!("ok: launched {}", pkg))
        }
        "terminate" => {
            let pkg = arg(args, 0, "terminate \"Device\" com.example.app")?;
            p.shell(&["am", "force-stop", pkg]).await?;
            Ok(format!("ok: stopped {}", pkg))
        }
        "navigate" => {
            let url = arg(args, 0, "navigate \"Device\" \"https://…\"")?;
            let quoted = format!("'{}'", url.replace('\'', "'\\''"));
            let out = p.shell(&["am", "start", "-a", "android.intent.action.VIEW", "-d", &quoted]).await?;
            if out.contains("Error") {
                return Err(out);
            }
            Ok(format!("ok: opened {}", url))
        }
        "fill" | "hover" | "focus" | "select" | "uncheck" | "selectall" | "clear" | "scrollintoview" | "text"
        | "html" | "evaluate" | "wait" | "logs-start" | "logs" | "resize" | "ua" | "check" => Err(format!(
            "'{}' is a browser-portal command — on a device use snapshot or screenshot, then click, type, key, scroll, swipe, button, launch, terminate or navigate",
            verb
        )),
        _ => Err(format!("unknown portal command '{}' — run `narrater help portal`", verb)),
    }
}

/// Streams the device screen to the tile while it's attached.
pub fn start_stream(app: AppHandle, node: String, p: Arc<DevicePortal>) {
    if p.streaming.swap(true, Ordering::Relaxed) {
        return;
    }
    tokio::spawn(async move {
        let mut last = 0u64;
        while p.streaming.load(Ordering::Relaxed) {
            let started = Instant::now();
            match p.screencap().await {
                Ok(png) => {
                    let mut h = std::collections::hash_map::DefaultHasher::new();
                    png.hash(&mut h);
                    let hash = h.finish();
                    if hash != last {
                        last = hash;
                        let screen = p.screen();
                        let frame = tokio::task::spawn_blocking(move || encode_frame(&png)).await;
                        if let Ok(Ok(data)) = frame {
                            let _ = app.emit("portal_frame", FrameEvent {
                                id: node.clone(),
                                data,
                                width: screen.0 as f64,
                                height: screen.1 as f64,
                            });
                        }
                    }
                }
                Err(_) => tokio::time::sleep(Duration::from_secs(1)).await,
            }
            if let Some(rest) = FRAME_INTERVAL.checked_sub(started.elapsed()) {
                tokio::time::sleep(rest).await;
            }
        }
    });
}

pub fn stop_stream(p: &DevicePortal) {
    p.streaming.store(false, Ordering::Relaxed);
}

/// PNG screenshot → downscaled JPEG, base64 (the tile only needs a preview).
fn encode_frame(png: &[u8]) -> Result<String, String> {
    use base64::Engine;
    let img = image::load_from_memory_with_format(png, image::ImageFormat::Png).map_err(|e| e.to_string())?;
    let img = if img.height() > FRAME_MAX_HEIGHT {
        img.resize(u32::MAX, FRAME_MAX_HEIGHT, image::imageops::FilterType::Triangle)
    } else {
        img
    };
    let mut out = Vec::new();
    let encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, FRAME_QUALITY);
    img.to_rgb8().write_with_encoder(encoder).map_err(|e| e.to_string())?;
    Ok(base64::engine::general_purpose::STANDARD.encode(out))
}

/// Raw input from the tile, in device pixels.
pub async fn input(p: &DevicePortal, kind: &str, params: &serde_json::Value) -> Result<(), String> {
    let n = |k: &str| params[k].as_f64().unwrap_or(0.0) as i32;
    match kind {
        "tap" => p.tap((n("x"), n("y"))).await,
        "swipe" => p.swipe([n("x1"), n("y1"), n("x2"), n("y2")], params["ms"].as_u64().unwrap_or(300) as u32).await,
        "key" => {
            let name = params["key"].as_str().unwrap_or("");
            match android::keycode(name) {
                Some(code) => p.keyevent(code).await,
                None if name.chars().count() == 1 => p.type_text(name).await,
                None => Ok(()),
            }
        }
        "text" => p.type_text(params["text"].as_str().unwrap_or("")).await,
        _ => Err("unsupported device input".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_downscale_to_jpeg() {
        let img = image::RgbImage::from_pixel(540, 1200, image::Rgb([200, 30, 30]));
        let mut png = Vec::new();
        image::DynamicImage::ImageRgb8(img)
            .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
            .unwrap();
        use base64::Engine;
        let jpeg = base64::engine::general_purpose::STANDARD.decode(encode_frame(&png).unwrap()).unwrap();
        let back = image::load_from_memory(&jpeg).unwrap();
        assert_eq!(back.height(), FRAME_MAX_HEIGHT);
        assert_eq!(back.width(), 432);
    }

    #[tokio::test]
    async fn refs_and_points() {
        let p = DevicePortal {
            serial: "emulator-5554".into(),
            refs: Mutex::new(vec![Bounds { x: 0, y: 0, w: 100, h: 50 }]),
            streaming: AtomicBool::new(false),
            screen: Mutex::new((1080, 2400)),
        };
        assert_eq!(p.point("@e1"), Ok((50, 25)));
        assert_eq!(p.point("300,400"), Ok((300, 400)));
        assert!(p.point("@e9").unwrap_err().contains("snapshot again"));
        assert!(p.point("#css").unwrap_err().contains("@refs"));
        // Browser-only verbs are refused, not silently accepted
        assert!(run_verb(&p, "Pixel", "fill", &["@e1".into(), "x".into()]).await.unwrap_err().contains("browser-portal command"));
        assert!(run_verb(&p, "Pixel", "check", &["@e1".into()]).await.unwrap_err().contains("browser-portal"));
    }
}
