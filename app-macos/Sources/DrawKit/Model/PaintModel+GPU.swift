// The resident layer stack and the upload path that keeps it in step with the core.

import SwiftUI

extension PaintModel {
    // MARK: GPU canvas

    /// The resident layer path: the canvas keeps every layer on the GPU in both ranges and in
    /// every colour space.
    var gpuEligible: Bool {
        gpuCanvas && !layers.isEmpty
            && layers.allSatisfy { $0.kind != .group && !$0.clipping }
    }

    /// The live stroke path. It commits encoded ink into the tile, which only matches the stored
    /// premultiplied eight-bit pixels; an extended-range document still paints on the CPU.
    var gpuLiveStroke: Bool {
        gpuEligible && !hdr
    }

    var gpuNeedsRebuild: Bool { gpuLayersDirty }
    var gpuSpecsNeedUpdate: Bool { gpuSpecsDirty }

    func setGPUCanvasAvailable(_ available: Bool) {
        guard gpuCanvas != available else { return }
        gpuCanvas = available
        gpuLayersDirty = true
        gpuSpecsDirty = true
        if !available {
            // The CPU pixels were not maintained while the GPU drew; rebuild them before any
            // fallback present or export.
            markSurfaceFull()
        }
    }

    func markGPURebuild() {
        gpuLayersDirty = true
        gpuSpecsDirty = true
    }

    func gpuUploadsComplete() {
        gpuLayersDirty = false
        gpuSpecsDirty = false
    }

    func gpuSpecsUpdated() {
        gpuSpecsDirty = false
    }

    /// The rectangle of this layer that still has to reach the GPU, consumed on read.
    func takeGPUDirty(index: Int) -> (x: Int, y: Int, width: Int, height: Int)? {
        var rect = [UInt32](repeating: 0, count: 4)
        let found = rect.withUnsafeMutableBufferPointer {
            dk_take_gpu_dirty(handle, UInt32(index), $0.baseAddress)
        }
        guard found != 0, rect[2] > 0, rect[3] > 0 else { return nil }
        return (Int(rect[0]), Int(rect[1]), Int(rect[2]), Int(rect[3]))
    }

    /// Straight RGBA8 pixels of one layer rectangle, handed to the surface for upload.
    func withLayerPixels(index: Int, x: Int, y: Int, width: Int, height: Int,
                         _ body: (UnsafePointer<UInt8>) -> Bool) -> Bool {
        guard width > 0, height > 0 else { return false }
        var pixels = [UInt8](repeating: 0, count: width * height * 4)
        let copied = pixels.withUnsafeMutableBytes { bytes in
            dk_layer_rect(handle, UInt32(index), UInt32(max(0, x)), UInt32(max(0, y)),
                          UInt32(width), UInt32(height),
                          bytes.bindMemory(to: UInt8.self).baseAddress) != 0
        }
        guard copied else { return false }
        return pixels.withUnsafeBytes { bytes in
            guard let base = bytes.bindMemory(to: UInt8.self).baseAddress else { return false }
            return body(base)
        }
    }

    /// Straight linear Display P3 values of one layer rectangle, for an extended-range
    /// resident stack.
    func withLayerLinear(index: Int, x: Int, y: Int, width: Int, height: Int,
                         _ body: (UnsafePointer<Float>) -> Bool) -> Bool {
        guard width > 0, height > 0 else { return false }
        var pixels = [Float](repeating: 0, count: width * height * 4)
        let copied = pixels.withUnsafeMutableBufferPointer { values in
            dk_layer_rect_linear(handle, UInt32(index), UInt32(max(0, x)), UInt32(max(0, y)),
                                 UInt32(width), UInt32(height), values.baseAddress) != 0
        }
        guard copied else { return false }
        return pixels.withUnsafeBufferPointer { values in
            guard let base = values.baseAddress else { return false }
            return body(base)
        }
    }

    /// Upload a layer rectangle in whatever form the surface composes: half-float linear for an
    /// extended-range drawable, straight eight-bit otherwise.
    func uploadLayerRect(_ surface: PaintSurface, index: Int, x: Int, y: Int,
                         width: Int, height: Int) -> Bool {
        if surface.extended {
            return withLayerLinear(index: index, x: x, y: y, width: width, height: height) {
                surface.uploadLayerLinear(index: index, x: x, y: y, width: width, height: height,
                                          pixels: $0)
            }
        }
        return withLayerPixels(index: index, x: x, y: y, width: width, height: height) {
            surface.uploadLayer(index: index, x: x, y: y, width: width, height: height,
                                pixels: $0)
        }
    }

    func gpuLayerState(index: Int)
        -> (opacity: Float, blend: Int32, visible: Bool, fill: (Float, Float, Float, Float)?)? {
        guard index >= 0, index < layers.count else { return nil }
        let layer = layers[index]
        let opacity = dk_layer_effective_opacity(handle, UInt32(index))
        let visible = dk_layer_effective_visible(handle, UInt32(index)) != 0
        var fill: (Float, Float, Float, Float)?
        if layer.kind == .fill {
            var values = [Float](repeating: 0, count: 4)
            if dk_layer_fill(handle, UInt32(index), &values) != 0 {
                fill = (values[0], values[1], values[2], values[3])
            }
        }
        return (opacity, Int32(layer.blend.rawValue), visible, fill)
    }

    /// Rebuild the CPU composite after a GPU-only session; exports and cached thumbnails use it.
    func refreshCPUImage() {
        _ = dk_composite(handle, pixels.pointer)
        if let linear = linearPixels {
            _ = dk_composite_linear(handle, linear.pointer)
        }
        image = makeImage()
    }
}
