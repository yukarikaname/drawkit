// DrawKit's app icon, drawn so the mark in the window and the mark in the Dock are the same idea:
// two offset tiles and one brush stroke on a warm ink ground.
//
// Run with an output .iconset directory; the bundle script then turns it into AppIcon.icns.

import AppKit

let arguments = CommandLine.arguments
let output = arguments.count > 1 ? arguments[1] : "AppIcon.iconset"
try? FileManager.default.createDirectory(atPath: output, withIntermediateDirectories: true)

func draw(_ size: Int) -> Data? {
    guard let rep = NSBitmapImageRep(
        bitmapDataPlanes: nil, pixelsWide: size, pixelsHigh: size,
        bitsPerSample: 8, samplesPerPixel: 4, hasAlpha: true, isPlanar: false,
        colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0
    ) else { return nil }

    NSGraphicsContext.saveGraphicsState()
    NSGraphicsContext.current = NSGraphicsContext(bitmapImageRep: rep)
    let s = CGFloat(size)

    let plate = NSRect(x: s * 0.055, y: s * 0.055, width: s * 0.89, height: s * 0.89)
    let platePath = NSBezierPath(roundedRect: plate, xRadius: s * 0.20, yRadius: s * 0.20)
    NSGradient(
        starting: NSColor(srgbRed: 0.145, green: 0.133, blue: 0.120, alpha: 1),
        ending: NSColor(srgbRed: 0.086, green: 0.078, blue: 0.070, alpha: 1)
    )?.draw(in: platePath, angle: -90)
    NSColor.white.withAlphaComponent(0.07).setStroke()
    platePath.lineWidth = s * 0.006
    platePath.stroke()

    // The back tile is a kit tray: an outline, quiet under the paint.
    let back = NSRect(x: s * 0.235, y: s * 0.395, width: s * 0.345, height: s * 0.345)
    let backPath = NSBezierPath(roundedRect: back, xRadius: s * 0.075, yRadius: s * 0.075)
    NSColor(srgbRed: 0.58, green: 0.55, blue: 0.50, alpha: 0.9).setStroke()
    backPath.lineWidth = s * 0.020
    backPath.stroke()

    // The front tile is the paint.
    let front = NSRect(x: s * 0.425, y: s * 0.265, width: s * 0.345, height: s * 0.345)
    let frontPath = NSBezierPath(roundedRect: front, xRadius: s * 0.075, yRadius: s * 0.075)
    NSGraphicsContext.saveGraphicsState()
    let shadow = NSShadow()
    shadow.shadowColor = NSColor.black.withAlphaComponent(0.45)
    shadow.shadowBlurRadius = s * 0.035
    shadow.shadowOffset = NSSize(width: 0, height: -s * 0.012)
    shadow.set()
    NSGradient(
        starting: NSColor(srgbRed: 0.929, green: 0.396, blue: 0.294, alpha: 1),
        ending: NSColor(srgbRed: 0.769, green: 0.224, blue: 0.149, alpha: 1)
    )?.draw(in: frontPath, angle: -90)
    NSGraphicsContext.restoreGraphicsState()

    // One stroke across the tile, the line that makes it a painting.
    let stroke = NSBezierPath()
    stroke.move(to: NSPoint(x: s * 0.250, y: s * 0.280))
    stroke.curve(
        to: NSPoint(x: s * 0.760, y: s * 0.750),
        controlPoint1: NSPoint(x: s * 0.450, y: s * 0.560),
        controlPoint2: NSPoint(x: s * 0.560, y: s * 0.470)
    )
    stroke.lineWidth = s * 0.072
    stroke.lineCapStyle = .round
    NSColor(srgbRed: 0.965, green: 0.945, blue: 0.914, alpha: 1).setStroke()
    stroke.stroke()

    NSGraphicsContext.restoreGraphicsState()
    return rep.representation(using: .png, properties: [:])
}

let sizes: [(String, Int)] = [
    ("icon_16x16.png", 16),
    ("icon_16x16@2x.png", 32),
    ("icon_32x32.png", 32),
    ("icon_32x32@2x.png", 64),
    ("icon_128x128.png", 128),
    ("icon_128x128@2x.png", 256),
    ("icon_256x256.png", 256),
    ("icon_256x256@2x.png", 512),
    ("icon_512x512.png", 512),
    ("icon_512x512@2x.png", 1024),
]

for (name, size) in sizes {
    guard let data = draw(size) else {
        fatalError("could not draw the icon")
    }
    try data.write(to: URL(fileURLWithPath: output).appendingPathComponent(name))
}
