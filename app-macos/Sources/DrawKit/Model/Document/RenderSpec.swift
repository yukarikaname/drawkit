// The output a stroke renderer targets. A stroke stores its radius as a plain number, and the
// tape also stores which reference that number uses, so the output can either honour the drawing
// or force one policy for the whole export.

import CoreGraphics
import Foundation

enum BrushScale: Int {
    /// Follow the document: a 20 px line becomes 5 px at 25% output.
    case canvas = 0
    /// Stay the same size in output pixels: a 20 px line stays 20 px at any output.
    case screen = 1

    var title: LocalizedStringResource {
        switch self {
        case .canvas: "Follow canvas"
        case .screen: "Fixed on screen"
        }
    }
}

/// What an export does with the radius, overriding what each stroke recorded.
enum RenderThickness {
    case recorded
    case canvas
    case screen

    var title: LocalizedStringResource {
        switch self {
        case .recorded: "As drawn"
        case .canvas: "Follow canvas"
        case .screen: "Fixed on screen"
        }
    }
}

struct OutputSpec {
    let width: Int
    let height: Int
    /// Output pixels per document unit. One for a full-size render.
    let pixelsPerUnit: CGFloat
    let thickness: RenderThickness

    static func canvas(width: Int, height: Int) -> OutputSpec {
        OutputSpec(width: width, height: height, pixelsPerUnit: 1, thickness: .recorded)
    }

    /// The output for a document of `width` x `height` units at a percentage of its size.
    static func scaled(
        width: Int, height: Int, percent: Int,
        thickness: RenderThickness
    ) -> OutputSpec {
        let factor = CGFloat(max(percent, 1)) / 100
        return OutputSpec(
            width: max(1, Int((CGFloat(width) * factor).rounded())),
            height: max(1, Int((CGFloat(height) * factor).rounded())),
            pixelsPerUnit: factor, thickness: thickness)
    }

    /// A document point in output pixels.
    func point(_ x: Float, _ y: Float) -> (Float, Float) {
        let scale = Float(pixelsPerUnit)
        return (x * scale, y * scale)
    }

    /// A recorded radius in output pixels. `recorded` is the policy stored with the stroke.
    func radius(_ radius: Float, recorded: BrushScale) -> Float {
        let policy: BrushScale
        switch thickness {
        case .recorded: policy = recorded
        case .canvas: policy = .canvas
        case .screen: policy = .screen
        }
        switch policy {
        case .canvas: return radius * Float(pixelsPerUnit)
        case .screen: return radius
        }
    }

    /// A pixel count that belongs to the painting (fill close/expand/feather).
    func pixels(_ count: UInt32) -> UInt32 {
        let scaled = (CGFloat(count) * pixelsPerUnit).rounded()
        return UInt32(max(0, min(scaled, CGFloat(UInt32.max))))
    }
}
