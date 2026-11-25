// The launch-screen mark: the same two tiles and brush stroke as the app icon, on a transparent
// ground so the launch colour supplies the ink. Run with an output PNG path.

import AppKit

let output = CommandLine.arguments.count > 1 ? CommandLine.arguments[1] : "LaunchLogo.png"
let size = 768
guard let rep = NSBitmapImageRep(
    bitmapDataPlanes: nil, pixelsWide: size, pixelsHigh: size,
    bitsPerSample: 8, samplesPerPixel: 4, hasAlpha: true, isPlanar: false,
    colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0
) else {
    fatalError("could not create the launch logo")
}

NSGraphicsContext.saveGraphicsState()
NSGraphicsContext.current = NSGraphicsContext(bitmapImageRep: rep)
let s = CGFloat(size)

// The back tile is a quiet outline; the front tile is the paint.
let back = NSRect(x: s * 0.16, y: s * 0.39, width: s * 0.37, height: s * 0.37)
let backPath = NSBezierPath(roundedRect: back, xRadius: s * 0.08, yRadius: s * 0.08)
NSColor(srgbRed: 0.965, green: 0.945, blue: 0.914, alpha: 0.55).setStroke()
backPath.lineWidth = s * 0.030
backPath.stroke()

let front = NSRect(x: s * 0.47, y: s * 0.24, width: s * 0.37, height: s * 0.37)
let frontPath = NSBezierPath(roundedRect: front, xRadius: s * 0.08, yRadius: s * 0.08)
NSColor(srgbRed: 0.886, green: 0.337, blue: 0.243, alpha: 1).setFill()
frontPath.fill()

let stroke = NSBezierPath()
stroke.move(to: NSPoint(x: s * 0.18, y: s * 0.20))
stroke.curve(
    to: NSPoint(x: s * 0.82, y: s * 0.80),
    controlPoint1: NSPoint(x: s * 0.46, y: s * 0.54),
    controlPoint2: NSPoint(x: s * 0.56, y: s * 0.44)
)
stroke.lineWidth = s * 0.062
stroke.lineCapStyle = .round
NSColor(srgbRed: 0.965, green: 0.945, blue: 0.914, alpha: 1).setStroke()
stroke.stroke()

NSGraphicsContext.restoreGraphicsState()
guard let data = rep.representation(using: .png, properties: [:]) else {
    fatalError("could not encode the launch logo")
}
try data.write(to: URL(fileURLWithPath: output))
