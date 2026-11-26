// The resident layer stack: what the surface still has to upload and how each layer composites.

extension Studio: CanvasPresenterSource {
    var layerCount: Int { layers.count }
    var canvasWidth: Int { width }
    var canvasHeight: Int { height }
    var needsRebuild: Bool { gpuRebuild }

    func rebuildCompleted() {
        gpuRebuild = false
    }

    /// The rectangle of this layer the surface still has to upload, consumed on read.
    func takeDirtyRect(index: Int) -> (x: Int, y: Int, width: Int, height: Int)? {
        var rect = [UInt32](repeating: 0, count: 4)
        let found = rect.withUnsafeMutableBufferPointer {
            dk_take_gpu_dirty(handle, UInt32(index), $0.baseAddress)
        }
        guard found != 0, rect[2] > 0, rect[3] > 0 else { return nil }
        return (Int(rect[0]), Int(rect[1]), Int(rect[2]), Int(rect[3]))
    }

    /// Straight RGBA8 pixels of one layer rectangle.
    private func withLayerPixels(index: Int, x: Int, y: Int, width: Int, height: Int,
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

    /// Straight linear Display P3 values of one layer rectangle, for an extended-range stack.
    private func withLayerLinear(index: Int, x: Int, y: Int, width: Int, height: Int,
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

    /// Upload a layer rectangle in the form the surface composes.
    func uploadLayer(_ surface: PaintSurface, index: Int, x: Int, y: Int,
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

    /// The paint or fill layer a clipped layer shows through, or -1 when it is not clipped.
    func gpuLayerClip(index: Int) -> Int {
        guard index >= 0, index < layers.count,
              dk_layer_clipping(handle, UInt32(index)) != 0 else { return -1 }
        return Int(dk_layer_clip_base(handle, UInt32(index)))
    }

    /// Push every layer's parameters to the surface. The surface ignores a value that is already
    /// current, so this is safe to call on every present.
    func updateLayerSpecs(_ surface: PaintSurface) {
        for index in 0..<layers.count {
            var fill: (Float, Float, Float, Float)?
            if dk_layer_kind(handle, UInt32(index)) == 2 {
                var values = [Float](repeating: 0, count: 4)
                if dk_layer_fill(handle, UInt32(index), &values) != 0 {
                    fill = (values[0], values[1], values[2], values[3])
                }
            }
            surface.setLayer(index: index,
                             opacity: dk_layer_opacity(handle, UInt32(index)),
                             blend: Int(dk_layer_blend(handle, UInt32(index))),
                             visible: dk_layer_visible(handle, UInt32(index)) != 0,
                             fill: fill)
            surface.setLayerParent(index: index, parent: dk_layer_parent(handle, UInt32(index)))
            surface.setLayerClip(index: index, base: Int32(gpuLayerClip(index: index)))
        }
    }

    /// Working colour to straight linear Display P3, for an extended-range live stroke.
    func linearInk(_ ink: (Float, Float, Float)) -> (Float, Float, Float) {
        var values = [Float](repeating: 0, count: 3)
        let converted = values.withUnsafeMutableBufferPointer {
            dk_linear_colour(handle, ink.0, ink.1, ink.2, $0.baseAddress)
        }
        guard converted != 0 else { return ink }
        return (values[0], values[1], values[2])
    }
}
