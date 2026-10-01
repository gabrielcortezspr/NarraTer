//! Page-side pieces of portal automation: the JavaScript evaluated in the
//! page (snapshot refs, element lookup) and key/selector parsing.

use serde_json::{json, Value};

/// Helpers available to every action script as `N`. Elements are found
/// across open shadow roots; `@eN` refs are attributes set by the last
/// snapshot.
const HELPERS: &str = r#"{
  roots() {
    const out = [document];
    const walk = (r) => r.querySelectorAll('*').forEach((el) => {
      if (el.shadowRoot) { out.push(el.shadowRoot); walk(el.shadowRoot); }
    });
    walk(document);
    return out;
  },
  find(sel) {
    if (sel.startsWith('@')) {
      const ref = sel.slice(1);
      for (const r of this.roots()) {
        const el = r.querySelector(`[data-narrater-ref="${CSS.escape(ref)}"]`);
        if (el) return el;
      }
      throw new Error(`no element ${sel} — refs change when the page does; run snapshot again`);
    }
    for (const r of this.roots()) {
      const el = r.querySelector(sel);
      if (el) return el;
    }
    throw new Error(`no element matches ${sel}`);
  },
  center(el) {
    el.scrollIntoView({ block: 'center', inline: 'center', behavior: 'instant' });
    const b = el.getBoundingClientRect();
    return { x: b.left + b.width / 2, y: b.top + b.height / 2 };
  },
  active() {
    let a = document.activeElement;
    while (a && a.shadowRoot && a.shadowRoot.activeElement) a = a.shadowRoot.activeElement;
    return a;
  },
  fire(el, ...types) {
    for (const t of types) el.dispatchEvent(new Event(t, { bubbles: true }));
  },
}"#;

/// Accessibility-ish snapshot: interactive elements (and headings for
/// context) with refs, name, extras, box and state flags.
pub const SNAPSHOT: &str = r#"(N) => {
  for (const r of N.roots()) r.querySelectorAll('[data-narrater-ref]').forEach((e) => e.removeAttribute('data-narrater-ref'));
  const SEL = 'a[href],button,input:not([type=hidden]),select,textarea,summary,[role=button],[role=link],[role=checkbox],[role=radio],[role=tab],[role=menuitem],[role=option],[role=switch],[role=textbox],[role=combobox],[role=searchbox],[role=slider],[contenteditable=""],[contenteditable=true],[onclick],[tabindex]:not([tabindex="-1"]),h1,h2,h3';
  const MAX = 500;
  const active = N.active();
  const lines = [];
  let n = 0;
  outer: for (const r of N.roots()) {
    for (const el of r.querySelectorAll(SEL)) {
      if (n >= MAX) break outer;
      const b = el.getBoundingClientRect();
      if (b.width < 1 || b.height < 1) continue;
      const st = getComputedStyle(el);
      if (st.visibility === 'hidden' || st.display === 'none' || st.opacity === '0') continue;
      const tag = el.tagName.toLowerCase();
      const isField = tag === 'input' || tag === 'textarea' || tag === 'select';
      const label = el.labels && el.labels[0] ? el.labels[0].innerText : '';
      const name = (el.getAttribute('aria-label') || label || el.getAttribute('placeholder') || el.getAttribute('alt')
        || el.getAttribute('title') || (isField ? '' : el.innerText) || '').replace(/\s+/g, ' ').trim().slice(0, 80);
      const ref = 'e' + (++n);
      el.setAttribute('data-narrater-ref', ref);
      const extra = [];
      if (tag === 'input') extra.push('type=' + (el.type || 'text'));
      if (tag === 'a' && el.href) extra.push('href=' + el.href.slice(0, 120));
      if (isField && el.value && el.type !== 'password' && !['checkbox', 'radio', 'submit', 'button'].includes(el.type)) {
        extra.push('value=' + JSON.stringify(String(el.value).slice(0, 60)));
      }
      const flags = [];
      if (el === active) flags.push('*focused*');
      if (el.checked) flags.push('*checked*');
      if (el.disabled) flags.push('*disabled*');
      if (b.bottom < 0 || b.top > innerHeight || b.right < 0 || b.left > innerWidth) flags.push('*offscreen*');
      lines.push(`@${ref} ${el.getAttribute('role') || tag} ${JSON.stringify(name)}`
        + (extra.length ? ' ' + extra.join(' ') : '')
        + ` [${Math.round(b.left)},${Math.round(b.top)} ${Math.round(b.width)}x${Math.round(b.height)}]`
        + (flags.length ? ' ' + flags.join(' ') : ''));
    }
  }
  return `viewport: ${innerWidth}x${innerHeight}  url: ${location.href}  title: ${document.title}\n`
    + (lines.length ? lines.join('\n') : '(no interactive elements)')
    + (n >= MAX ? `\n(… truncated at ${MAX} elements)` : '');
}"#;

pub const CENTER: &str = "(N, A) => N.center(N.find(A.sel))";
pub const FOCUS: &str = "(N, A) => { const el = N.find(A.sel); el.focus(); return true; }";
pub const TEXT: &str = "(N, A) => { const el = N.find(A.sel); return (el.innerText ?? el.textContent ?? '').trim(); }";
pub const SCROLL_INTO_VIEW: &str =
    "(N, A) => { N.find(A.sel).scrollIntoView({ block: 'center', behavior: 'instant' }); return true; }";
/// Focus (optional selector) and select everything in the focused field.
pub const SELECT_ALL: &str = r#"(N, A) => {
  const el = A.sel ? N.find(A.sel) : N.active();
  if (!el) throw new Error('nothing is focused — pass a selector');
  el.focus();
  if (typeof el.select === 'function') el.select();
  else document.execCommand('selectAll');
  return true;
}"#;
/// Empties a field (or the focused one) the way user editing would.
pub const CLEAR: &str = r#"(N, A) => {
  const el = A.sel ? N.find(A.sel) : N.active();
  if (!el) throw new Error('nothing is focused — pass a selector');
  el.focus();
  if ('value' in el) { el.value = ''; N.fire(el, 'input', 'change'); }
  else if (el.isContentEditable) { el.textContent = ''; N.fire(el, 'input'); }
  return true;
}"#;
pub const SELECT_OPTION: &str = r#"(N, A) => {
  const el = N.find(A.sel);
  if (el.tagName.toLowerCase() !== 'select') throw new Error(`${A.sel} is not a <select>`);
  const want = A.option.trim().toLowerCase();
  const opt = [...el.options].find((o) => o.text.trim().toLowerCase() === want || o.value.toLowerCase() === want);
  if (!opt) throw new Error(`no option "${A.option}" — options: ${[...el.options].map((o) => o.text.trim()).join(', ')}`);
  el.value = opt.value;
  N.fire(el, 'input', 'change');
  return opt.text.trim();
}"#;
pub const IS_CHECKED: &str = r#"(N, A) => {
  const el = N.find(A.sel);
  const aria = el.getAttribute('aria-checked');
  return 'checked' in el ? !!el.checked : aria === 'true';
}"#;
pub const EXISTS: &str = r#"(N, A) => {
  try {
    const el = N.find(A.sel);
    const b = el.getBoundingClientRect();
    return b.width > 0 && b.height > 0;
  } catch { return false; }
}"#;
pub const HTML: &str = "() => document.documentElement.outerHTML";
pub const INFO: &str = "() => `url: ${location.href}\\ntitle: ${document.title}\\nviewport: ${innerWidth}x${innerHeight}`";
pub const VIEWPORT: &str = "() => `viewport: ${innerWidth}x${innerHeight}`";

/// The Runtime.evaluate expression running `func(N, args)`.
pub fn action_expression(func: &str, args: &Value) -> String {
    format!("(() => {{ const N = {}; const A = {}; return ({})(N, A); }})()", HELPERS, args, func)
}

/// What a command argument designates.
#[derive(Debug, PartialEq)]
pub enum Target {
    Point(f64, f64),
    Selector(String),
}

/// `@e3` (snapshot ref), `#submit` (CSS) or `350,200` (viewport coordinates).
pub fn parse_target(s: &str) -> Target {
    let t = s.trim();
    if let Some((x, y)) = t.split_once(',') {
        if let (Ok(x), Ok(y)) = (x.trim().parse::<f64>(), y.trim().parse::<f64>()) {
            return Target::Point(x, y);
        }
    }
    Target::Selector(t.to_string())
}

/// A key press for Input.dispatchKeyEvent.
#[derive(Debug, PartialEq)]
pub struct KeySpec {
    pub key: String,
    pub code: String,
    pub key_code: u32,
    /// Text typed by the key (Enter submits forms only when it carries "\r").
    pub text: Option<String>,
    /// CDP modifiers bitmask: Alt=1, Ctrl=2, Meta=4, Shift=8.
    pub modifiers: u32,
    /// Editing commands for shortcuts (headless Chrome won't run them itself).
    pub commands: Vec<&'static str>,
}

/// Parses "Enter", "Tab", "ArrowUp", "a", "ctrl+a", "cmd+c", "shift+tab"…
pub fn parse_key(spec: &str) -> Result<KeySpec, String> {
    let parts: Vec<&str> = spec.split('+').map(str::trim).filter(|p| !p.is_empty()).collect();
    let (base, mods) = parts.split_last().ok_or("empty key")?;
    let mut modifiers = 0;
    for m in mods {
        modifiers |= match m.to_lowercase().as_str() {
            "alt" | "option" => 1,
            "ctrl" | "control" => 2,
            "cmd" | "meta" | "super" => 4,
            "shift" => 8,
            other => return Err(format!("unknown modifier '{}'", other)),
        };
    }
    let named: Option<(&str, &str, u32, Option<&str>)> = match base.to_lowercase().as_str() {
        "enter" | "return" => Some(("Enter", "Enter", 13, Some("\r"))),
        "tab" => Some(("Tab", "Tab", 9, None)),
        "escape" | "esc" => Some(("Escape", "Escape", 27, None)),
        "backspace" => Some(("Backspace", "Backspace", 8, None)),
        "delete" | "del" => Some(("Delete", "Delete", 46, None)),
        "space" => Some((" ", "Space", 32, Some(" "))),
        "arrowup" | "up" => Some(("ArrowUp", "ArrowUp", 38, None)),
        "arrowdown" | "down" => Some(("ArrowDown", "ArrowDown", 40, None)),
        "arrowleft" | "left" => Some(("ArrowLeft", "ArrowLeft", 37, None)),
        "arrowright" | "right" => Some(("ArrowRight", "ArrowRight", 39, None)),
        "home" => Some(("Home", "Home", 36, None)),
        "end" => Some(("End", "End", 35, None)),
        "pageup" => Some(("PageUp", "PageUp", 33, None)),
        "pagedown" => Some(("PageDown", "PageDown", 34, None)),
        _ => None,
    };
    let mut k = if let Some((key, code, key_code, text)) = named {
        KeySpec { key: key.into(), code: code.into(), key_code, text: text.map(String::from), modifiers, commands: vec![] }
    } else {
        let mut chars = base.chars();
        let (Some(c), None) = (chars.next(), chars.next()) else {
            return Err(format!("unknown key '{}' — use Enter, Tab, Escape, Backspace, Delete, Space, Arrow*, Home, End, PageUp/Down or one character", base));
        };
        let upper = c.to_ascii_uppercase();
        let code = if c.is_ascii_alphabetic() {
            format!("Key{}", upper)
        } else if c.is_ascii_digit() {
            format!("Digit{}", c)
        } else {
            String::new()
        };
        let shifted = modifiers & 8 != 0;
        let key = if shifted && c.is_ascii_alphabetic() { upper.to_string() } else { c.to_string() };
        KeySpec {
            text: (modifiers & !8 == 0).then(|| key.clone()),
            key,
            code,
            key_code: upper as u32,
            modifiers,
            commands: vec![],
        }
    };
    // ctrl/cmd shortcuts → editing commands
    if modifiers & 6 != 0 {
        k.commands = match k.key.to_lowercase().as_str() {
            "a" => vec!["selectAll"],
            "c" => vec!["copy"],
            "x" => vec!["cut"],
            "v" => vec!["paste"],
            "z" if modifiers & 8 != 0 => vec!["redo"],
            "z" => vec!["undo"],
            _ => vec![],
        };
    }
    Ok(k)
}

/// keyDown/keyUp params for Input.dispatchKeyEvent.
pub fn key_events(k: &KeySpec) -> [Value; 2] {
    let mut down = json!({
        "type": if k.text.is_some() { "keyDown" } else { "rawKeyDown" },
        "key": k.key, "code": k.code,
        "windowsVirtualKeyCode": k.key_code, "nativeVirtualKeyCode": k.key_code,
        "modifiers": k.modifiers,
    });
    if let Some(t) = &k.text {
        down["text"] = json!(t);
        down["unmodifiedText"] = json!(t);
    }
    if !k.commands.is_empty() {
        down["commands"] = json!(k.commands);
    }
    let up = json!({
        "type": "keyUp", "key": k.key, "code": k.code,
        "windowsVirtualKeyCode": k.key_code, "nativeVirtualKeyCode": k.key_code,
        "modifiers": k.modifiers,
    });
    [down, up]
}

/// User agents for `portal ua <preset>` (None = the browser default).
pub fn ua_preset(name: &str) -> Option<Option<&'static str>> {
    Some(match name {
        "ios" => Some("Mozilla/5.0 (iPhone; CPU iPhone OS 18_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.0 Mobile/15E148 Safari/604.1"),
        "android" => Some("Mozilla/5.0 (Linux; Android 15; Pixel 9) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/140.0.0.0 Mobile Safari/537.36"),
        "firefox-android" => Some("Mozilla/5.0 (Android 15; Mobile; rv:140.0) Gecko/140.0 Firefox/140.0"),
        "edge-android" => Some("Mozilla/5.0 (Linux; Android 15; Pixel 9) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/140.0.0.0 Mobile Safari/537.36 EdgA/140.0.0.0"),
        "chrome" => Some("Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/140.0.0.0 Safari/537.36"),
        "firefox" => Some("Mozilla/5.0 (X11; Linux x86_64; rv:140.0) Gecko/20100101 Firefox/140.0"),
        "edge" => Some("Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/140.0.0.0 Safari/537.36 Edg/140.0.0.0"),
        "desktop" => None,
        _ => return None,
    })
}

pub const UA_PRESETS: &str = "ios, android, firefox-android, edge-android, chrome, firefox, edge, desktop";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn targets_parse_refs_css_and_points() {
        assert_eq!(parse_target("@e3"), Target::Selector("@e3".into()));
        assert_eq!(parse_target("#submit"), Target::Selector("#submit".into()));
        assert_eq!(parse_target("350,200"), Target::Point(350.0, 200.0));
        assert_eq!(parse_target(" 10.5, 20 "), Target::Point(10.5, 20.0));
        // A CSS selector list is not a point
        assert_eq!(parse_target("a,b"), Target::Selector("a,b".into()));
    }

    #[test]
    fn keys_parse_named_chars_and_shortcuts() {
        let enter = parse_key("Enter").unwrap();
        assert_eq!((enter.key.as_str(), enter.key_code, enter.text.as_deref()), ("Enter", 13, Some("\r")));
        let a = parse_key("a").unwrap();
        assert_eq!((a.code.as_str(), a.text.as_deref(), a.modifiers), ("KeyA", Some("a"), 0));
        let sel = parse_key("ctrl+a").unwrap();
        assert_eq!((sel.modifiers, sel.text.clone(), sel.commands.clone()), (2, None, vec!["selectAll"]));
        let st = parse_key("shift+tab").unwrap();
        assert_eq!((st.key.as_str(), st.modifiers), ("Tab", 8));
        assert_eq!(parse_key("shift+a").unwrap().text.as_deref(), Some("A"));
        assert_eq!(parse_key("cmd+shift+z").unwrap().commands, vec!["redo"]);
        assert!(parse_key("hyper+x").is_err());
        assert!(parse_key("F13key").is_err());
    }

    #[test]
    fn key_events_use_raw_down_for_non_text_keys() {
        let [down, up] = key_events(&parse_key("Tab").unwrap());
        assert_eq!(down["type"], "rawKeyDown");
        assert_eq!(up["type"], "keyUp");
        let [down, _] = key_events(&parse_key("Enter").unwrap());
        assert_eq!(down["type"], "keyDown");
        assert_eq!(down["text"], "\r");
    }

    #[test]
    fn ua_presets() {
        assert!(ua_preset("ios").unwrap().unwrap().contains("iPhone"));
        assert_eq!(ua_preset("desktop"), Some(None));
        assert_eq!(ua_preset("nokia"), None);
    }

    #[test]
    fn action_expression_wraps_helpers_and_args() {
        let e = action_expression(CENTER, &json!({ "sel": "@e1" }));
        assert!(e.starts_with("(() => { const N = {"));
        assert!(e.contains(r#"const A = {"sel":"@e1"};"#));
    }
}
