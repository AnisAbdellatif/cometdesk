// Draws the CometDesk logo (a monitor with a pointer) and writes the app icon assets:
//   swift tools/gen_icon.swift <out-dir>
// -> <out-dir>/AppIcon.iconset/*.png (run `iconutil -c icns` on it) and <out-dir>/logo-256.png.
import AppKit
import CoreGraphics

func rgb(_ hex: UInt32, _ a: CGFloat = 1) -> CGColor {
    CGColor(red: CGFloat((hex >> 16) & 255) / 255, green: CGFloat((hex >> 8) & 255) / 255,
            blue: CGFloat(hex & 255) / 255, alpha: a)
}
let space = CGColorSpaceCreateDeviceRGB()
func gradient(_ a: UInt32, _ b: UInt32) -> CGGradient {
    CGGradient(colorsSpace: space, colors: [rgb(a), rgb(b)] as CFArray, locations: [0, 1])!
}

func draw(_ ctx: CGContext, size: Int) {
    // 1024-unit canvas, y down.
    ctx.scaleBy(x: CGFloat(size) / 1024, y: -CGFloat(size) / 1024)
    ctx.translateBy(x: 0, y: -1024)

    // Squircle tile.
    let tile = CGPath(roundedRect: CGRect(x: 100, y: 100, width: 824, height: 824),
                      cornerWidth: 190, cornerHeight: 190, transform: nil)
    ctx.saveGState()
    ctx.addPath(tile); ctx.clip()
    ctx.drawLinearGradient(gradient(0x242E55, 0x090D1B), start: CGPoint(x: 512, y: 100),
                           end: CGPoint(x: 512, y: 924), options: [])
    // Soft glow behind the screen.
    let glow = CGGradient(colorsSpace: space,
                          colors: [rgb(0x5B8CFF, 0.28), rgb(0x5B8CFF, 0)] as CFArray, locations: [0, 1])!
    ctx.drawRadialGradient(glow, startCenter: CGPoint(x: 512, y: 470), startRadius: 0,
                           endCenter: CGPoint(x: 512, y: 470), endRadius: 420, options: [])
    ctx.restoreGState()
    ctx.addPath(tile)
    ctx.setStrokeColor(rgb(0xFFFFFF, 0.10)); ctx.setLineWidth(4); ctx.strokePath()

    ctx.translateBy(x: 0, y: -12)
    let accent = gradient(0x4FE3C1, 0x5B8CFF)
    func fillGradient(_ path: CGPath, stroke: CGFloat? = nil) {
        ctx.saveGState()
        ctx.addPath(path)
        if let w = stroke {
            ctx.setLineWidth(w); ctx.replacePathWithStrokedPath()
        }
        ctx.clip()
        ctx.drawLinearGradient(accent, start: CGPoint(x: 232, y: 280), end: CGPoint(x: 792, y: 790),
                               options: [.drawsBeforeStartLocation, .drawsAfterEndLocation])
        ctx.restoreGState()
    }

    // Screen: dark fill, gradient frame.
    let screen = CGPath(roundedRect: CGRect(x: 232, y: 292, width: 560, height: 372),
                        cornerWidth: 70, cornerHeight: 70, transform: nil)
    ctx.addPath(screen); ctx.setFillColor(rgb(0x0C1226, 0.85)); ctx.fillPath()
    fillGradient(screen, stroke: 44)
    // Stand.
    fillGradient(CGPath(rect: CGRect(x: 474, y: 676, width: 76, height: 62), transform: nil))
    fillGradient(CGPath(roundedRect: CGRect(x: 372, y: 738, width: 280, height: 44),
                        cornerWidth: 22, cornerHeight: 22, transform: nil))

    // Pointer.
    let tip = CGPoint(x: 426, y: 380)
    let pts: [(CGFloat, CGFloat)] = [(0, 0), (0, 196), (46, 152), (82, 234), (118, 218), (82, 138), (146, 138)]
    let arrow = CGMutablePath()
    for (i, p) in pts.enumerated() {
        let q = CGPoint(x: tip.x + p.0, y: tip.y + p.1)
        i == 0 ? arrow.move(to: q) : arrow.addLine(to: q)
    }
    arrow.closeSubpath()
    ctx.setLineJoin(.round)
    ctx.addPath(arrow); ctx.setStrokeColor(rgb(0x0C1226)); ctx.setLineWidth(26); ctx.strokePath()
    ctx.addPath(arrow); ctx.setFillColor(rgb(0xFFFFFF)); ctx.fillPath()
    ctx.addPath(arrow); ctx.setStrokeColor(rgb(0xFFFFFF)); ctx.setLineWidth(10); ctx.strokePath()
}

func png(size: Int) -> Data {
    let ctx = CGContext(data: nil, width: size, height: size, bitsPerComponent: 8, bytesPerRow: 0,
                        space: space, bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)!
    ctx.interpolationQuality = .high
    draw(ctx, size: size)
    return NSBitmapImageRep(cgImage: ctx.makeImage()!).representation(using: .png, properties: [:])!
}

let out = URL(fileURLWithPath: CommandLine.arguments.dropFirst().first ?? ".")
let set = out.appendingPathComponent("AppIcon.iconset")
try? FileManager.default.createDirectory(at: set, withIntermediateDirectories: true)
for base in [16, 32, 128, 256, 512] {
    try! png(size: base).write(to: set.appendingPathComponent("icon_\(base)x\(base).png"))
    try! png(size: base * 2).write(to: set.appendingPathComponent("icon_\(base)x\(base)@2x.png"))
}
try! png(size: 256).write(to: out.appendingPathComponent("logo-256.png"))
