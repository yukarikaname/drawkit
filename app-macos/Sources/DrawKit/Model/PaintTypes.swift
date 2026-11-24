// Value types the model hands to the views and the core.

import SwiftUI
import AppKit

struct Ink {
    var red: Float = 0.12
    var green: Float = 0.12
    var blue: Float = 0.14
    var alpha: Float = 1.0

    /// `glow` is stops above paper white for HDR documents; zero leaves the picked colour alone.
    static func from(_ color: Color, space: PaintColorSpace = .sRGB, glow: Double = 0) -> Ink {
        let ns = NSColor(color).usingColorSpace(space.nsColorSpace)
            ?? NSColor(color).usingColorSpace(.sRGB) ?? .black
        let gain = Float(pow(2.0, max(glow, 0.0)))
        return Ink(red: Float(ns.redComponent) * gain, green: Float(ns.greenComponent) * gain,
                   blue: Float(ns.blueComponent) * gain, alpha: Float(ns.alphaComponent))
    }
}

struct StatusMessage: Equatable {
    let text: String
    let isError: Bool
}

struct SurfaceRect {
    var x: Int
    var y: Int
    var width: Int
    var height: Int
}

enum LayerKind: Int {
    case paint = 0
    case group = 1
    case fill = 2
}

struct LayerInfo: Identifiable, Equatable {
    let id: Int
    let name: String
    let visible: Bool
    let opacity: Double
    let kind: LayerKind
    let blend: BlendMode
    let locked: Bool
    let alphaLocked: Bool
    let clipping: Bool
    let parent: Int?
    let expanded: Bool
    let colorTag: Int
}

/// The pixels the canvas draws. Owned here, never copied: `dk_composite` writes straight into it,
/// and the `CGImage` points at it through a data provider.
final class PixelBuffer {
    let pointer: UnsafeMutablePointer<UInt8>
    let count: Int

    init(count: Int) {
        self.count = count
        pointer = .allocate(capacity: count)
        pointer.initialize(repeating: 0, count: count)
    }

    deinit {
        pointer.deinitialize(count: count)
        pointer.deallocate()
    }
}
