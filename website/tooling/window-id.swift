// Prints the CoreGraphics window id for an application's main window.
//
//     swift window-id.swift Rotero
//
// `screencapture -l` takes a CoreGraphics window number. AppleScript's
// `window id` is a different namespace and does not work with it, which is why
// this reads the window list directly.
//
// Exits 1 if no matching on-screen window is found.

import CoreGraphics
import Foundation

let owner = CommandLine.arguments.count > 1 ? CommandLine.arguments[1] : "Rotero"

let options = CGWindowListOption(arrayLiteral: .optionOnScreenOnly, .excludeDesktopElements)
guard let windows = CGWindowListCopyWindowInfo(options, kCGNullWindowID) as? [[String: Any]] else {
    FileHandle.standardError.write("cannot read the window list\n".data(using: .utf8)!)
    exit(1)
}

// Menu bars and other chrome share the owner name, so take the largest window,
// which is the document window in every case that matters here.
let candidates = windows.compactMap { window -> (Int, CGFloat)? in
    guard let name = window[kCGWindowOwnerName as String] as? String, name == owner,
          let number = window[kCGWindowNumber as String] as? Int,
          let bounds = window[kCGWindowBounds as String] as? [String: CGFloat],
          let width = bounds["Width"], let height = bounds["Height"],
          width > 200, height > 200
    else { return nil }
    return (number, width * height)
}

guard let best = candidates.max(by: { $0.1 < $1.1 }) else {
    FileHandle.standardError.write("no on-screen window owned by \(owner)\n".data(using: .utf8)!)
    exit(1)
}

print(best.0)
