// The wgpu swapchain over a CAMetalLayer, shared by the macOS window and the iPad view.
//
// The core asks for either eight-bit encoded pixels or premultiplied linear floats; the Rust
// surface owns the texture and the shader. This type only owns the handle's lifetime and turns
// rectangles into the primitives the C API takes.

import CoreGraphics
import Foundation
import QuartzCore

@_silgen_name("dk_canvas_surface_supports_layers")
func dk_canvas_surface_supports_layers(_ handle: UnsafeMutableRawPointer?) -> Int32
@_silgen_name("dk_canvas_surface_reset_layers")
func dk_canvas_surface_reset_layers(
    _ handle: UnsafeMutableRawPointer?, _ count: UInt32, _ width: UInt32, _ height: UInt32)
@_silgen_name("dk_canvas_surface_upload_layer")
func dk_canvas_surface_upload_layer(
    _ handle: UnsafeMutableRawPointer?, _ index: UInt32,
    _ x: UInt32, _ y: UInt32, _ width: UInt32, _ height: UInt32,
    _ pixels: UnsafePointer<UInt8>?) -> Int32
@_silgen_name("dk_canvas_surface_set_layer")
func dk_canvas_surface_set_layer(
    _ handle: UnsafeMutableRawPointer?, _ index: UInt32, _ opacity: Float,
    _ blend: Int32, _ visible: Int32, _ fill: Int32,
    _ red: Float, _ green: Float, _ blue: Float, _ alpha: Float)
@_silgen_name("dk_canvas_surface_present_layers")
func dk_canvas_surface_present_layers(
    _ handle: UnsafeMutableRawPointer?,
    _ paperX: Float, _ paperY: Float, _ paperWidth: Float, _ paperHeight: Float) -> Int32

/// Linear light pixels for an extended-range canvas: four floats per pixel. The GPU surface
/// converts the dirty rectangle to halves.
final class LinearBuffer {
    let pointer: UnsafeMutablePointer<Float>
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

final class PaintSurface {
    private let layer: CAMetalLayer
    private var handle: UnsafeMutableRawPointer?
    private(set) var width: Int
    private(set) var height: Int
    /// True when the machine really took the extended linear Display P3 drawable.
    private(set) var extended: Bool

    /// `width` and `height` are drawable pixels, not points. `extended` asks for `Rgba16Float`.
    init?(layer: CAMetalLayer, width: Int, height: Int, extended: Bool,
          colorSpace: PaintColorSpace) {
        guard width > 0, height > 0 else { return nil }
        self.layer = layer
        self.width = width
        self.height = height
        self.extended = false
        handle = dk_canvas_surface_new(
            Unmanaged.passUnretained(layer).toOpaque(),
            UInt32(width), UInt32(height), extended ? 1 : 0, Int32(colorSpace.rawValue))
        guard let handle else { return nil }
        self.extended = dk_canvas_surface_hdr(handle) != 0
    }

    deinit {
        dk_canvas_surface_free(handle)
    }

    /// Display exposure in stops; it changes the preview, not the document.
    func setExposure(_ exposure: Float) {
        dk_canvas_surface_set_exposure(handle, exposure)
    }

    func resize(width: Int, height: Int) {
        guard width > 0, height > 0, width != self.width || height != self.height else { return }
        self.width = width
        self.height = height
        dk_canvas_surface_resize(handle, UInt32(width), UInt32(height))
    }

    /// Whether this surface can composite resident layers itself.
    var supportsLayers: Bool {
        guard let handle else { return false }
        return dk_canvas_surface_supports_layers(handle) != 0
    }

    func resetLayers(count: Int, width: Int, height: Int) {
        guard let handle else { return }
        dk_canvas_surface_reset_layers(
            handle, UInt32(max(0, count)), UInt32(max(1, width)), UInt32(max(1, height)))
    }

    func uploadLayer(index: Int, x: Int, y: Int, width: Int, height: Int,
                     pixels: UnsafePointer<UInt8>?) -> Bool {
        guard let handle, width > 0, height > 0 else { return false }
        return dk_canvas_surface_upload_layer(
            handle, UInt32(index), UInt32(max(0, x)), UInt32(max(0, y)),
            UInt32(width), UInt32(height), pixels) != 0
    }

    func setLayer(index: Int, opacity: Float, blend: Int, visible: Bool,
                  fill: (Float, Float, Float, Float)?) {
        guard let handle else { return }
        dk_canvas_surface_set_layer(
            handle, UInt32(index), opacity, Int32(blend), visible ? 1 : 0,
            fill == nil ? 0 : 1,
            fill?.0 ?? 0, fill?.1 ?? 0, fill?.2 ?? 0, fill?.3 ?? 0)
    }

    /// Composite resident layers on the GPU. 1 drawn, 2 composed but the window was not
    /// presentable, 0 the layer path cannot run.
    func presentLayers(paper: CGRect) -> Int {
        guard let handle else { return 0 }
        return Int(dk_canvas_surface_present_layers(
            handle, Float(paper.minX), Float(paper.minY),
            Float(paper.width), Float(paper.height)))
    }

    /// Present the painting. `paper` is in drawable pixels with the origin at the top left;
    /// `dirty` is in canvas pixels. `linear` selects the `f32` buffer and the `Rgba16Float` texture.
    func present(pixels: UnsafeMutableRawPointer?, width: Int, height: Int,
                 paper: CGRect, dirty: CGRect?, linear: Bool) -> Bool {
        guard let handle else { return false }
        let region = dirty ?? .zero
        let args = (
            UInt32(max(0, region.minX)), UInt32(max(0, region.minY)),
            UInt32(max(0, region.width)), UInt32(max(0, region.height))
        )
        if linear {
            return dk_canvas_surface_present_hdr(
                handle, pixels?.assumingMemoryBound(to: Float.self),
                UInt32(width), UInt32(height),
                Float(paper.minX), Float(paper.minY),
                Float(paper.width), Float(paper.height),
                args.0, args.1, args.2, args.3) != 0
        }
        return dk_canvas_surface_present(
            handle, pixels?.assumingMemoryBound(to: UInt8.self),
            UInt32(width), UInt32(height),
            Float(paper.minX), Float(paper.minY),
            Float(paper.width), Float(paper.height),
            args.0, args.1, args.2, args.3) != 0
    }
}
