---
name: narrater-portal-devices
description: Drive an Android emulator or physical Android phone on the NarraTer canvas — tap, type, swipe, press hardware buttons, launch apps, take screenshots and read the accessibility tree. Use inside NarraTer when the user asks to run or test a mobile app rather than a web page.
---

# NarraTer Android device automation

You're running inside NarraTer, a spatial canvas that connects AI agents, terminals, notes and portals. The `narrater` CLI is on your PATH (or use `"$NARRATER_CLI"`).

Device portals show an Android emulator or a physical Android phone (over adb) on the canvas. You can tap, type, scroll, press hardware buttons, launch apps, take screenshots and read the accessibility tree — the user watches the same screen in the portal. The portal name is always required; only portals connected to you are reachable (`narrater list`).

## Opening a device

`narrater portal devices` lists connected phones and running emulators (marked free or in use), plus the Android Virtual Devices managed by Android Studio.

`narrater portal create --simulator ID ["Name"]` opens a portal on a device and connects it to your terminal. `ID` is a device serial (`emulator-5554`, a phone's serial) or an AVD name — a stopped AVD is booted headless, which takes a minute; the next command on the portal waits until it's ready. Prefer a device marked free. Several portals can share one running device.

Android emulator creation, deletion and resource settings belong in Android Studio — NarraTer never changes an AVD's settings. A physical phone must already have USB debugging enabled and have authorized this computer.

## Snapshot and refs

`narrater portal snapshot "Pixel"` is the most important command. It returns the foreground app's accessibility tree with refs:

```
app: com.example.app  screen: 1080x2400pt  elements: 3
@e1 text "Checkout" [420,90 240x60]
@e2 field "Search" [60,240 960x120] *focused*
@e3 button "Submit" [720,420 300x120]
```

The `pt` label is kept for compatibility; the numbers are Android device pixels. Commands accept an `@ref` from the latest snapshot or `x,y` device coordinates.

## Commands

### Interaction
- `narrater portal click "Pixel" @e3` — tap the element's center (`tap` is an alias)
- `narrater portal click "Pixel" 540,1200` — tap coordinates
- `narrater portal type "Pixel" "hello world"` — type into the focused field (`type "Pixel" @e2 "…"` taps it first). ASCII only.
- `narrater portal key "Pixel" Enter` — Enter, Tab, Backspace, Delete, Space, Arrow*, and `back`
- `narrater portal scroll "Pixel" down 400` — swipe-scroll up, down, left or right (optionally at `@ref` or `x,y`)
- `narrater portal swipe "Pixel" @e3 @e9` — one completed swipe (`drag` holds longer)
- `narrater portal button "Pixel" home` — hardware buttons: `back`, `home`, `recents`, `menu`, `power`, `volumeup`, `volumedown` (also `home "Pixel"`, `lock "Pixel"`)

### Apps and capture
- `narrater portal launch "Pixel" com.example.app` — launch an installed package
- `narrater portal terminate "Pixel" com.example.app` — stop a package
- `narrater portal navigate "Pixel" "https://example.com"` — open a URL through Android
- `narrater portal screenshot "Pixel"` — PNG; prints its path (read it as an image)
- `narrater portal info "Pixel"` — device identity, Android version, display size, foreground app

`snapshot` can take about two seconds (it uses Android's `uiautomator`). Run it again after the screen changes to refresh refs. Prefer snapshot + refs for interaction; use screenshot for visual layout.

Browser-only commands (fill, evaluate, html…) are refused on a device portal — use snapshot or screenshot, then click, type, key, scroll, swipe, button or launch.

## Recommended workflow

1. `narrater portal snapshot "Pixel"` — read the screen, get refs.
2. Interact with refs, coordinates, text and keys.
3. Snapshot again to verify.
4. Refresh the snapshot whenever refs go stale.

For Expo or React Native, pass the device serial that `narrater portal info "Pixel"` reports to the project's Android run command.
