// The output a stroke renderer targets. A stroke stores its radius as a plain number, so the
// number only means one thing once the output says what it is relative to: document units or
// output pixels. `BrushScale` is that reference.

import CoreGraphics

enum BrushScale {
    /// Follow the document: a 20 px line becomes 5 px at 25% output.
    case canvas
    /// Stay the same size in output pixels: a 20 px line stays 20 px at any output.
    case screen
}

struct OutputSpec {
    let width: Int
    let height: Int
    /// Output pixels per document unit. One for a full-size render.
    let pixelsPerUnit: CGFloat
    let brushScale: BrushScale

    static func canvas(width: Int, height: Int) -> OutputSpec {
        OutputSpec(width: width, height: height, pixelsPerUnit: 1, brushScale: .canvas)
    }

    /// The output for a document of `width` x `height` units at a percentage of its size.
    static func scaled(width: Int, height: Int, percent: Int,
                       brush: BrushScale) -> OutputSpec {
        let factor = CGFloat(max(percent, 1)) / 100
        return OutputSpec(
            width: max(1, Int((CGFloat(width) * factor).rounded())),
            height: max(1, Int((CGFloat(height) * factor).rounded())),
            pixelsPerUnit: factor, brushScale: brush)
    }

    /// A document point in output pixels.
    func point(_ x: Float, _ y: Float) -> (Float, Float) {
        let scale = Float(pixelsPerUnit)
        return (x * scale, y * scale)
    }

    /// A recorded brush radius in output pixels, per the thickness policy.
    func radius(_ radius: Float) -> Float {
        switch brushScale {
        case .canvas: radius * Float(pixelsPerUnit)
        case .screen: radius
        }
    }

    /// A pixel count that belongs to the painting (fill close/expand/feather).
    func pixels(_ count: UInt32) -> UInt32 {
        let scaled = (CGFloat(count) * pixelsPerUnit).rounded()
        return UInt32(max(0, min(scaled, CGFloat(UInt32.max))))
    }
}
