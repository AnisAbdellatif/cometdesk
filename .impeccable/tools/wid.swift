import CoreGraphics
for w in CGWindowListCopyWindowInfo(.optionAll, 0) as! [[String:Any]] {
  if let o = w[kCGWindowOwnerName as String] as? String, o.lowercased().contains("cometdesk") { print(w[kCGWindowNumber as String]!, w[kCGWindowBounds as String]!, w[kCGWindowIsOnscreen as String] ?? "") }
}
