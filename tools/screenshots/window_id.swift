// Prints the window number of the largest on-screen window owned by a process: window_id <pid>.
// With a second argument ("any"), floating windows count too (the menu-bar window floats).
// screencapture -l needs it; there is no command-line way to get it otherwise.
import CoreGraphics
import Foundation

let pid = Int32(CommandLine.arguments[1])!
let list = CGWindowListCopyWindowInfo([.optionOnScreenOnly], kCGNullWindowID) as? [[String: Any]] ?? []
let anyLayer = CommandLine.arguments.count > 2
let mine = list.filter { ($0[kCGWindowOwnerPID as String] as? Int32) == pid && (anyLayer || ($0[kCGWindowLayer as String] as? Int) == 0) }
let area = { (w: [String: Any]) -> Double in
  let b = w[kCGWindowBounds as String] as? [String: Double] ?? [:]
  return (b["Width"] ?? 0) * (b["Height"] ?? 0)
}
if let w = mine.max(by: { area($0) < area($1) }), let n = w[kCGWindowNumber as String] as? Int { print(n) }
