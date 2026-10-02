//! Android-side pieces of device portals that don't touch a device: parsing
//! `adb devices -l` and `uiautomator dump`, key codes, `input text` quoting.

/// One line of `adb devices -l`.
#[derive(Debug, PartialEq, Clone)]
pub struct AdbDevice {
    pub serial: String,
    /// "device" (online), "offline", "unauthorized", "authorizing"…
    pub state: String,
    pub model: Option<String>,
}

impl AdbDevice {
    pub fn is_emulator(&self) -> bool {
        self.serial.starts_with("emulator-")
    }
}

pub fn parse_devices(out: &str) -> Vec<AdbDevice> {
    out.lines()
        .skip_while(|l| !l.starts_with("List of devices"))
        .skip(1)
        .filter_map(|l| {
            let mut parts = l.split_whitespace();
            let serial = parts.next()?.to_string();
            let state = parts.next()?.to_string();
            let model = parts.find_map(|p| p.strip_prefix("model:")).map(|m| m.replace('_', " "));
            Some(AdbDevice { serial, state, model })
        })
        .collect()
}

/// `Physical size: 1080x2400` (an `Override size:` line wins).
pub fn parse_wm_size(out: &str) -> Option<(u32, u32)> {
    let pick = |prefix: &str| {
        out.lines().find_map(|l| l.trim().strip_prefix(prefix)).and_then(|s| {
            let (w, h) = s.trim().split_once('x')?;
            Some((w.trim().parse().ok()?, h.trim().parse().ok()?))
        })
    };
    pick("Override size:").or_else(|| pick("Physical size:"))
}

/// An element of a snapshot: what `@eN` points at.
#[derive(Debug, PartialEq, Clone, Copy)]
pub struct Bounds {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl Bounds {
    pub fn center(&self) -> (i32, i32) {
        (self.x + self.w / 2, self.y + self.h / 2)
    }
}

/// `[x1,y1][x2,y2]`
fn parse_bounds(s: &str) -> Option<Bounds> {
    let nums: Vec<i32> = s
        .split(|c: char| !c.is_ascii_digit() && c != '-')
        .filter(|p| !p.is_empty())
        .filter_map(|p| p.parse().ok())
        .collect();
    let [x1, y1, x2, y2] = nums[..] else { return None };
    (x2 > x1 && y2 > y1).then_some(Bounds { x: x1, y: y1, w: x2 - x1, h: y2 - y1 })
}

/// Element kind from the Android widget class.
fn kind_of(class: &str, clickable: bool) -> &'static str {
    let short = class.rsplit('.').next().unwrap_or(class);
    match short {
        s if s.contains("EditText") || s.contains("AutoComplete") => "field",
        s if s.contains("CheckBox") => "checkbox",
        s if s.contains("Switch") || s.contains("Toggle") => "switch",
        s if s.contains("RadioButton") => "radio",
        s if s.contains("Button") => "button",
        s if s.contains("Image") => "image",
        s if s.contains("TextView") || s.contains("Text") => "text",
        _ if clickable => "clickable",
        s if s.contains("RecyclerView") || s.contains("ScrollView") || s.contains("ListView") => "list",
        _ => "view",
    }
}

/// `uiautomator dump` XML → snapshot text + the refs' bounds (`@e1` is
/// index 0). Keeps elements a user would act on or read: clickable,
/// focusable, checkable or scrollable ones, and anything with text or a
/// content description.
pub fn snapshot(xml: &str, screen: (u32, u32)) -> Result<(String, Vec<Bounds>), String> {
    let start = xml.find("<?xml").or_else(|| xml.find("<hierarchy")).ok_or("no UI hierarchy in the dump")?;
    let end = xml.rfind("</hierarchy>").map(|i| i + "</hierarchy>".len()).unwrap_or(xml.len());
    let doc = roxmltree::Document::parse(&xml[start..end]).map_err(|e| format!("unreadable UI dump: {}", e))?;
    let mut lines = Vec::new();
    let mut refs = Vec::new();
    let mut app = None;
    for node in doc.descendants().filter(|n| n.has_tag_name("node")) {
        let attr = |k: &str| node.attribute(k).unwrap_or("");
        let flag = |k: &str| attr(k) == "true";
        if app.is_none() && !attr("package").is_empty() && attr("package") != "com.android.systemui" {
            app = Some(attr("package").to_string());
        }
        let Some(b) = parse_bounds(attr("bounds")) else { continue };
        let text = attr("text").trim();
        let desc = attr("content-desc").trim();
        let interesting = flag("clickable") || flag("focusable") || flag("checkable") || flag("scrollable")
            || !text.is_empty() || !desc.is_empty();
        if !interesting {
            continue;
        }
        // A clickable row usually has no text of its own — its children do
        let from_children = || -> String {
            node.descendants()
                .skip(1)
                .filter_map(|c| {
                    let t = c.attribute("text").map(str::trim).filter(|t| !t.is_empty());
                    t.or_else(|| c.attribute("content-desc").map(str::trim).filter(|t| !t.is_empty()))
                })
                .take(2)
                .collect::<Vec<_>>()
                .join(" · ")
        };
        let children = if text.is_empty() && desc.is_empty() && flag("clickable") { from_children() } else { String::new() };
        let resource = attr("resource-id").rsplit('/').next().unwrap_or("");
        let name: String = [text, desc, children.as_str(), resource]
            .into_iter()
            .find(|n| !n.is_empty())
            .unwrap_or("")
            .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(80)
        .collect();
        let mut flags = Vec::new();
        if flag("focused") {
            flags.push("*focused*");
        }
        if flag("checked") {
            flags.push("*checked*");
        }
        if !flag("enabled") {
            flags.push("*disabled*");
        }
        if flag("scrollable") {
            flags.push("*scrollable*");
        }
        refs.push(b);
        lines.push(format!(
            "@e{} {} {:?} [{},{} {}x{}]{}",
            refs.len(),
            kind_of(attr("class"), flag("clickable")),
            name,
            b.x,
            b.y,
            b.w,
            b.h,
            if flags.is_empty() { String::new() } else { format!(" {}", flags.join(" ")) }
        ));
    }
    // "pt" kept for wire compatibility with Maestri; the numbers are device pixels
    let header = format!(
        "app: {}  screen: {}x{}pt  elements: {}",
        app.as_deref().unwrap_or("?"),
        screen.0,
        screen.1,
        refs.len()
    );
    Ok((if lines.is_empty() { header } else { format!("{}\n{}", header, lines.join("\n")) }, refs))
}

/// Android key code for `key`/`button` names.
pub fn keycode(name: &str) -> Option<u32> {
    Some(match name.trim().to_lowercase().as_str() {
        "enter" | "return" => 66,
        "tab" => 61,
        "escape" | "esc" => 111,
        "backspace" => 67,
        "delete" | "del" => 112,
        "space" => 62,
        "arrowup" | "up" => 19,
        "arrowdown" | "down" => 20,
        "arrowleft" | "left" => 21,
        "arrowright" | "right" => 22,
        "back" => 4,
        "home" => 3,
        "recents" | "appswitch" => 187,
        "menu" => 82,
        "power" | "lock" | "side" => 26,
        "volumeup" => 24,
        "volumedown" => 25,
        "search" => 84,
        _ => return None,
    })
}

/// Arguments for `adb shell input text`: the device shell sees one quoted
/// word; spaces must be `%s` (input's own convention). Only ASCII is
/// supported by `input text`.
pub fn input_text_arg(text: &str) -> Result<String, String> {
    if !text.is_ascii() {
        return Err("Android's `input text` only types ASCII — for other characters, paste from the user or use an app intent".into());
    }
    let encoded = text.replace('%', "\\%").replace(' ', "%s");
    Ok(format!("'{}'", encoded.replace('\'', "'\\''")))
}

/// Swipe for `scroll <dir> <amount>`: content moves in `dir`, so the finger
/// travels the opposite way, centered on `at`.
pub fn scroll_swipe(dir: &str, amount: i32, at: (i32, i32), screen: (u32, u32)) -> Result<[i32; 4], String> {
    let (cx, cy) = at;
    let half = amount / 2;
    let clamp_x = |v: i32| v.clamp(1, screen.0 as i32 - 2);
    let clamp_y = |v: i32| v.clamp(1, screen.1 as i32 - 2);
    Ok(match dir {
        "down" => [cx, clamp_y(cy + half), cx, clamp_y(cy - half)],
        "up" => [cx, clamp_y(cy - half), cx, clamp_y(cy + half)],
        "right" => [clamp_x(cx + half), cy, clamp_x(cx - half), cy],
        "left" => [clamp_x(cx - half), cy, clamp_x(cx + half), cy],
        _ => return Err("scroll direction must be up, down, left or right".into()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn devices_list_parses() {
        let out = "* daemon started successfully\nList of devices attached\nemulator-5554          device product:sdk_gphone64_x86_64 model:sdk_gphone64_x86_64 device:emu64xa transport_id:1\nR58M12 unauthorized usb:1-1 transport_id:2\n\n";
        let d = parse_devices(out);
        assert_eq!(d.len(), 2);
        assert_eq!((d[0].serial.as_str(), d[0].state.as_str(), d[0].model.as_deref()), ("emulator-5554", "device", Some("sdk gphone64 x86 64")));
        assert!(d[0].is_emulator());
        assert_eq!((d[1].state.as_str(), d[1].model.clone()), ("unauthorized", None));
    }

    #[test]
    fn wm_size_prefers_override() {
        assert_eq!(parse_wm_size("Physical size: 1080x2400\n"), Some((1080, 2400)));
        assert_eq!(parse_wm_size("Physical size: 1080x2400\nOverride size: 720x1600\n"), Some((720, 1600)));
        assert_eq!(parse_wm_size("garbage"), None);
    }

    #[test]
    fn snapshot_lists_actionable_elements() {
        let xml = r#"UI hierchary dumped to: /dev/tty<?xml version='1.0' encoding='UTF-8' standalone='yes' ?><hierarchy rotation="0">
<node index="0" text="" resource-id="" class="android.widget.FrameLayout" package="com.example.app" content-desc="" checkable="false" checked="false" clickable="false" enabled="true" focusable="false" focused="false" scrollable="false" bounds="[0,0][1080,2400]">
  <node index="0" text="Checkout" resource-id="" class="android.widget.TextView" package="com.example.app" content-desc="" checkable="false" checked="false" clickable="false" enabled="true" focusable="false" focused="false" scrollable="false" bounds="[420,90][660,150]" />
  <node index="1" text="" resource-id="com.example.app:id/search" class="android.widget.EditText" package="com.example.app" content-desc="" checkable="false" checked="false" clickable="true" enabled="true" focusable="true" focused="true" scrollable="false" bounds="[60,240][1020,360]" />
  <node index="2" text="Submit" resource-id="" class="android.widget.Button" package="com.example.app" content-desc="" checkable="false" checked="false" clickable="true" enabled="false" focusable="true" focused="false" scrollable="false" bounds="[720,420][1020,540]" />
  <node index="3" text="" resource-id="" class="android.view.View" package="com.example.app" content-desc="" checkable="false" checked="false" clickable="false" enabled="true" focusable="false" focused="false" scrollable="false" bounds="[0,0][0,0]" />
  <node index="4" text="" resource-id="" class="android.widget.LinearLayout" package="com.example.app" content-desc="" checkable="false" checked="false" clickable="true" enabled="true" focusable="true" focused="false" scrollable="false" bounds="[0,600][1080,800]">
    <node index="0" text="Network &amp; internet" resource-id="" class="android.widget.TextView" package="com.example.app" content-desc="" checkable="false" checked="false" clickable="false" enabled="true" focusable="false" focused="false" scrollable="false" bounds="[200,620][600,680]" />
    <node index="1" text="Mobile, Wi-Fi" resource-id="" class="android.widget.TextView" package="com.example.app" content-desc="" checkable="false" checked="false" clickable="false" enabled="true" focusable="false" focused="false" scrollable="false" bounds="[200,680][600,740]" />
  </node>
</node></hierarchy>"#;
        let (text, refs) = snapshot(xml, (1080, 2400)).unwrap();
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines[0], "app: com.example.app  screen: 1080x2400pt  elements: 6");
        assert_eq!(lines[1], r#"@e1 text "Checkout" [420,90 240x60]"#);
        assert_eq!(lines[2], r#"@e2 field "search" [60,240 960x120] *focused*"#);
        assert_eq!(lines[3], r#"@e3 button "Submit" [720,420 300x120] *disabled*"#);
        assert_eq!(refs[2].center(), (870, 480));
        assert_eq!(lines[4], r#"@e4 clickable "Network & internet · Mobile, Wi-Fi" [0,600 1080x200]"#);
    }

    #[test]
    fn keys_and_text() {
        assert_eq!(keycode("Enter"), Some(66));
        assert_eq!(keycode("back"), Some(4));
        assert_eq!(keycode("recents"), Some(187));
        assert_eq!(keycode("nope"), None);
        assert_eq!(input_text_arg("hello world").unwrap(), "'hello%sworld'");
        assert_eq!(input_text_arg("it's 100%").unwrap(), r#"'it'\''s%s100\%'"#);
        assert!(input_text_arg("olá").is_err());
    }

    #[test]
    fn scroll_moves_the_finger_the_other_way() {
        assert_eq!(scroll_swipe("down", 400, (540, 1200), (1080, 2400)).unwrap(), [540, 1400, 540, 1000]);
        assert_eq!(scroll_swipe("up", 400, (540, 1200), (1080, 2400)).unwrap(), [540, 1000, 540, 1400]);
        // Clamped to the screen
        assert_eq!(scroll_swipe("down", 5000, (540, 1200), (1080, 2400)).unwrap()[1], 2398);
        assert!(scroll_swipe("sideways", 1, (0, 0), (10, 10)).is_err());
    }
}
