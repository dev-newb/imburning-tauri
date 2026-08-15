// Print CoreGraphics window IDs for on-screen windows, optionally filtered by
// owner name.
//
// Why this exists: WKWebView exposes no remote-debugging port, so unlike the
// Electron build there is no CDP `Page.captureScreenshot` for the Tauri
// window. The alternative — `screencapture -R<rect>` — captures a screen
// REGION, so it grabs whatever happens to be on top and forces you to raise
// the app first, stealing focus from whatever the user is doing.
//
// `screencapture -l<windowid>` captures a specific window even when it is
// unfocused, occluded, or behind other apps. It just needs the id, which is
// what this prints.
//
//   swiftc -O tools/winid.swift -o tools/winid
//   screencapture -x -o -l$(tools/winid imburning) shot.png
//
// Needs Screen Recording permission (the same grant `screencapture` already
// uses). Window *titles* are page-authored, so only owner names are matched.

import CoreGraphics
import Foundation

let filter = CommandLine.arguments.count > 1 ? CommandLine.arguments[1].lowercased() : nil
let verbose = CommandLine.arguments.contains("--list")

guard
    let windows = CGWindowListCopyWindowInfo(
        [.optionOnScreenOnly, .excludeDesktopElements], kCGNullWindowID
    ) as? [[String: Any]]
else {
    FileHandle.standardError.write(Data("cannot read the window list\n".utf8))
    exit(1)
}

var matches: [(id: Int, owner: String, name: String, w: Int, h: Int)] = []
for window in windows {
    guard
        let id = window[kCGWindowNumber as String] as? Int,
        let owner = window[kCGWindowOwnerName as String] as? String,
        let bounds = window[kCGWindowBounds as String] as? [String: Any],
        let width = bounds["Width"] as? Double,
        let height = bounds["Height"] as? Double
    else { continue }
    // Skip the 1x1 and menu-bar-sized helper windows every app carries.
    if width < 80 || height < 80 { continue }
    let name = (window[kCGWindowName as String] as? String) ?? ""
    if let filter, !owner.lowercased().contains(filter) { continue }
    matches.append((id, owner, name, Int(width), Int(height)))
}

if matches.isEmpty {
    FileHandle.standardError.write(Data("no window matched\n".utf8))
    exit(2)
}

if verbose || filter == nil {
    for m in matches {
        print("\(m.id)\t\(m.owner)\t\(m.w)x\(m.h)\t\(m.name)")
    }
} else {
    // Largest match — the main window, not a panel or tooltip.
    print(matches.max(by: { $0.w * $0.h < $1.w * $1.h })!.id)
}
