---
name: narrater-portal
description: Use a browser portal on the NarraTer canvas to navigate the web, interact with pages, fill forms, read content, capture screenshots and console logs. Use inside NarraTer when the user asks to browse a URL, test or check a web UI (e.g. a local dev server), or interact with a website.
---

# NarraTer portal browser automation

You're running inside NarraTer, a spatial canvas that connects AI agents, terminals, notes and browser portals. The `narrater` CLI is on your PATH (or use `"$NARRATER_CLI"`).

Portals are real browser tabs (Chrome) shown as nodes on the canvas — the user sees exactly what you do. You can navigate, click, type, take screenshots and inspect pages without any window focus. The portal name is always required; only portals connected to you are reachable — run `narrater list` to see them.

## Creating a portal

`narrater portal create URL ["Name"] [--size WxH]` opens a new portal next to your terminal, connected to you. Without a name it's called "Portal". `--size` sets the viewport (e.g. `--size 390x844` for mobile QA); default 1280x800.

```
narrater portal create http://localhost:3000
narrater portal create https://example.com "Docs"
narrater portal create http://localhost:5173 "Mobile" --size 390x844
```

`narrater portal edit "Portal" --url URL` points it elsewhere. `narrater portal close "Portal"` removes it — **destructive: only when the user explicitly asks**, never to tidy up.

## Snapshot and selectors

**`narrater portal snapshot "Portal"`** is the most important command: it lists the page's interactive elements (and headings) with **refs** you use as selectors:

```
viewport: 1280x800  url: https://example.com  title: Example
@e1 a "Home" href=https://example.com/ [10,5 60x20]
@e2 input "Search" type=text [200,50 300x32] *focused*
@e3 button "Submit" [510,50 80x32]
```

Selectors accepted everywhere: `@e3` (ref from the latest snapshot — most reliable), `#submit` (CSS), `350,200` (viewport coordinates). Refs go stale when the page changes: snapshot again.

## Commands

### Navigation
- `narrater portal navigate "Portal" "url"` — go to a URL (waits for load; prints url/title)
- `narrater portal info "Portal"` — url, title, viewport
- `narrater portal screenshot "Portal"` — PNG of the viewport; prints the file path (read it as an image)

### Interaction
- `narrater portal click "Portal" @e3` — real mouse click
- `narrater portal fill "Portal" @e2 "text"` — clear a field and type
- `narrater portal type "Portal" "text"` / `type "Portal" @e2 "text"` — append text
- `narrater portal key "Portal" "Enter"` — Enter, Tab, Escape, Backspace, Delete, Space, ArrowUp/Down/Left/Right, Home, End, PageUp/Down, one character; modifiers as `ctrl+a`, `shift+tab`, `cmd+c`
- `narrater portal hover "Portal" @e3` · `focus "Portal" @e2`
- `narrater portal select "Portal" @e5 "Option text"` — dropdown option
- `narrater portal check "Portal" @e6` / `uncheck` — checkbox (without a target, `check` is a peek: screenshot path)
- `narrater portal selectall "Portal" [@e2]` · `clear "Portal" [@e2]`
- `narrater portal drag "Portal" @e3 @e7`

### Scrolling
- `narrater portal scroll "Portal" down 300` — from the viewport center (up/down/left/right)
- `narrater portal scroll "Portal" down 300 @e5` — scroll the container under an element (or `x,y`)
- `narrater portal scrollintoview "Portal" @e10`

### Reading
- `narrater portal text "Portal" @e1` — element text
- `narrater portal html "Portal"` — full HTML
- `narrater portal evaluate "Portal" "document.title"` — run JavaScript (promises are awaited)
- `narrater portal wait "Portal" @e3 [timeoutMs]` — wait for an element (default 5000ms)

### Console
- `narrater portal logs-start "Portal"` — start capturing console output (once, after navigating)
- `narrater portal logs "Portal"` — read and clear the captured lines

### Responsive testing
- `narrater portal resize "Portal" 390 844` — exact CSS viewport (iPhone SE 375 667, iPhone 12 Pro 390 844, Pixel 7 412 915, iPad Mini 768 1024)
- `narrater portal ua "Portal" ios` — user agent preset + reload: `ios`, `android`, `firefox-android`, `edge-android`, `chrome`, `firefox`, `edge`, `desktop` (default). Omit the preset to read the current one. Pair with `resize` for device emulation.

## Recommended workflow

1. `narrater portal snapshot "Portal"` — understand the page, get refs
2. Act with refs: `click @e3`, `fill @e2 "value"`, `key "Enter"`
3. `snapshot` again to verify the result (or `screenshot` for visual layout)
4. Refs stale after a page change → snapshot again

Prefer snapshot + refs over screenshot + coordinates. If a command fails with a connection error, run `narrater debug`.
