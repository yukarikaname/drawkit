import Metal
import UIKit

/// The CAMetalLayer host. wgpu configures the layer; the view only keeps it the size of the canvas.
final class MetalCanvasView: UIView {
    override class var layerClass: AnyClass { CAMetalLayer.self }

    override init(frame: CGRect) {
        super.init(frame: frame)
        isOpaque = true
    }

    required init?(coder: NSCoder) {
        fatalError("init(coder:) is not used")
    }
}

/// Cursor and mirror guides above the presented painting. Transparent, and never takes touches.
final class CanvasOverlayView: UIView {
    weak var canvas: SketchCanvasView?

    override init(frame: CGRect) {
        super.init(frame: frame)
        isOpaque = false
        backgroundColor = .clear
    }

    required init?(coder: NSCoder) {
        fatalError("init(coder:) is not used")
    }

    override func draw(_ rect: CGRect) {
        guard let context = UIGraphicsGetCurrentContext() else { return }
        canvas?.drawChrome(context)
    }
}
