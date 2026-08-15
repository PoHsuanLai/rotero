// Debug helper: prints every on-screen window as "id<TAB>owner<TAB>title".
// Used to find the owner name to pass to window-id.swift.

import CoreGraphics
import Foundation

let options = CGWindowListOption(arrayLiteral: .optionOnScreenOnly, .excludeDesktopElements)
if let windows = CGWindowListCopyWindowInfo(options, kCGNullWindowID) as? [[String: Any]] {
    for window in windows {
        let owner = window[kCGWindowOwnerName as String] as? String ?? ""
        let title = window[kCGWindowName as String] as? String ?? ""
        let number = window[kCGWindowNumber as String] as? Int ?? 0
        let bounds = window[kCGWindowBounds as String] as? [String: CGFloat] ?? [:]
        let w = Int(bounds["Width"] ?? 0), h = Int(bounds["Height"] ?? 0)
        print("\(number)\t\(owner)\t\(title)\t\(w)x\(h)")
    }
}
