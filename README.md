# I'm Burning! — Tauri build

A port of [I'm Burning!](https://github.com/rmcquail/imburning) off Electron and onto
Tauri. Same widget, same UI, no bundled Chromium: on macOS it renders in WKWebView, on
Windows in WebView2, on Linux in WebKitGTK.

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

Both builds also read and write **the same** `config.json`
(`~/Library/Application Support/claude-usage-widget/` on macOS), so settings, hidden
rows and usage history carry over — run either build against one set of state.

## Two things the shim has to reproduce by hand

1. **Window dragging.** Electron gets it from `-webkit-app-region: drag` in the
   stylesheet, which this webview ignores. The shim reproduces the same regions with a
   `mousedown` handler calling `startDragging()`. If a drag rule moves in `styles.css`,
   update `DRAG_SELECTORS` in the shim to match.
2. **The external-URL allowlist.** The Electron preload enforced it; the shim enforces
   the identical list, so the renderer cannot open arbitrary URLs on either runtime.

## Status

Verified working against live accounts on macOS: the full UI renders, Antigravity usage
(with the "via Antigravity" chip and the default-hidden non-Gemini pool behind its
"1 hidden" chip), Codex CLI usage — matching the Electron build row for row — the
settings store, window controls and dragging, notifications, and the refresh loop.

Not yet ported:

* **The widget's own OAuth logins.** This build reads local CLI logins only. On a
  machine whose Anthropic usage comes from a claude.ai web login rather than
  `~/.claude/.credentials.json`, the Anthropic section stays empty.
* The detachable graph window, history export, the tray icon, the self-updater.
* Window presets (wide/tall) and `settingsRestore`, which still no-op.

Everything unported is stubbed to resolve to a benign value rather than throw, so a
missing feature leaves the widget running instead of blanking it.

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

Verified with Finder frontmost: the capture succeeded and focus never moved.
Pair it with `IMBURNING_DEV_REPORT=1`, which makes the shim report the rendered
DOM to `$TMPDIR/imburning-dev-report.txt`, and both builds can be verified
while you keep working.
