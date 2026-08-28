# I'm Burning! (Tauri)

A port of [I'm Burning! (Electron)](https://github.com/dev-newb/imburning-electron) off
Electron and onto Tauri. Same widget, same UI, no bundled Chromium: on macOS it renders
in WKWebView, on Windows in WebView2, on Linux in WebKitGTK.

![I'm Burning! (Tauri) — portrait view: Anthropic, OpenAI, and Google sections with account emails, banked-reset orb, and the prediction graph](docs/screenshot-portrait.png)

## Why

The Electron bundle ships a whole browser per copy. Tauri links against the webview the
OS already has, which is why the download is measured in single-digit megabytes rather
than hundreds. The tradeoff is that the *rendering engine differs per platform* — the
same CSS is WebKit on macOS, Chromium on Windows, WebKitGTK on Linux — so anything
relying on Chromium-only behaviour has to be checked on each.

## What is shared with the Electron build

The frontend is the Electron renderer, copied unmodified:

    ui/index.html, ui/app.js, ui/styles.css, ui/chart-utils.js, ui/graph.*

The only addition is `ui/tauri-shim.js`, which defines `window.electronAPI` in terms of
Tauri's `invoke()`. `app.js` cannot tell which runtime it is running on. Keeping the
renderer byte-identical means a fix in either repo can be copied across without a merge.

Each build keeps its **own** `config.json` (`imburning-tauri/` here,
`claude-usage-widget/` there), and this one copies the Electron settings and history in
on first run. Sharing one file was the first instinct and is wrong: both apps hold the
whole document in memory and rewrite it wholesale, so running them together lets one
silently discard the other's settings.

## Two things the shim has to reproduce by hand

1. **Window dragging.** Electron gets it from `-webkit-app-region: drag` in the
   stylesheet, which this webview ignores. The shim reproduces the same regions with a
   `mousedown` handler calling `startDragging()`. If a drag rule moves in `styles.css`,
   update `DRAG_SELECTORS` in the shim to match.
2. **The external-URL allowlist.** The Electron preload enforced it; the shim enforces
   the identical list, so the renderer cannot open arbitrary URLs on either runtime.

## Status

Verified against live accounts on macOS:

* usage for Antigravity, Gemini Code Assist, Codex and Claude Code
* claude.ai sign-in, and its API traffic through a webview (Cloudflare blocks plain
  HTTP clients — see `src/anthropic.rs`)
* OpenAI and Google sign-in over PKCE with a loopback redirect
* forecasts, the planner, frozen-provider detection and burn-spike alerts — the OpenAI
  and Google planner lines match the Electron build to the digit
* menu-bar badges, the detachable graph window, history export (CSV/JSON)
* settings, window controls, dragging, presets, compact mode, notifications, webhooks,
  alert sounds, and the chart over seeded history
* account email labels under each provider header, the banked-reset sound and its
  wide-mode orb, and a host-driven boot height fit (WKWebView throttles the renderer's
  own fit loop whenever the window isn't rendered — the host measures instead)

Not ported: the **self-updater**. Tauri's updater wants a signing key and a release
feed, which is release infrastructure rather than code, so it is a deliberate decision
rather than an oversight. `settingsRestore` is also still a no-op.

## Build

```bash
cd src-tauri && cargo build --release
```

Requires a Rust toolchain (`brew install rustup && rustup default stable`) and Xcode
command line tools.

## Testing without stealing focus

The Electron build can be driven headlessly over CDP
(`--remote-debugging-port=9222`), which screenshots and scripts the window
without focusing it. WKWebView has no equivalent port, and `screencapture
-R<rect>` grabs a screen *region* — so it captures whatever is on top and
forces you to raise the app first.

`tools/winid.swift` closes that gap. It prints CoreGraphics window ids, and
`screencapture -l<id>` captures that specific window even while it is
unfocused or fully occluded:

```bash
swiftc -O tools/winid.swift -o tools/winid
screencapture -x -o -l$(tools/winid imburning) shot.png
```

Two more pieces make verification genuinely non-disruptive:

* **`IMBURNING_NO_FOCUS=1`** runs the app under the Accessory activation
  policy, so the process cannot become active and launching it cannot take
  the front. Verified: frontmost app unchanged across a launch.
* **`IMBURNING_DEV_REPORT=1`** dumps the rendered DOM to
  `$TMPDIR/imburning-dev-report.txt`. The snapshot is PULLED from the Rust
  side, not pushed by the page on a timer — that distinction matters, see
  below.

macOS renders only the active Space, and that has two consequences. Pixel
capture of a window on another desktop returns nothing (`tools/winid <app>
--all` still finds it and reports geometry). And WebKit throttles the timers of
an unrendered window to a standstill, so anything the page schedules for itself
— including the auto-fit passes — simply stops. A page-driven report never
arrives and looks exactly like a broken frontend; a host-initiated
`eval_with_callback` still runs. Hence the pull.
