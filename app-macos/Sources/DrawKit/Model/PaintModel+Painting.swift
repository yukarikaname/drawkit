// Brush, fill, smudge and the GPU canvas bookkeeping.

import SwiftUI
import AppKit

extension PaintModel {
    // MARK: Painting

    /// Hand the live pixels to a GPU surface. The buffer is the same one the core composites into,
    /// so presenting does not copy or re-encode it.
    func withPixels(_ body: (UnsafeMutablePointer<UInt8>, Int, Int) -> Void) {
        body(pixels.pointer, width, height)
    }

    /// The buffer the display surface wants: linear floats for HDR, eight-bit for the rest. A
    /// surface that could not take the extended format asks for eight-bit even on an HDR document.
    func withSurfacePixels(hdr wantHdr: Bool,
                           _ body: (UnsafeMutableRawPointer, Int, Int, Bool) -> Void) {
        if wantHdr, needsLinear, let linear = linearPixels {
            body(UnsafeMutableRawPointer(linear.pointer), width, height, true)
        } else {
            body(UnsafeMutableRawPointer(pixels.pointer), width, height, false)
        }
    }

    // MARK: GPU canvas

    var gpuEligible: Bool {
        gpuCanvas && !hdr && !layers.isEmpty
            && layers.allSatisfy { $0.kind != .group && !$0.clipping }
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
        image = makeImage()
    }

    func setFillSnap(_ enabled: Bool) {
        fillSnap = enabled
        dk_set_fill_snap(handle, enabled ? 1 : 0)
    }

    func setStrokeDedupe(_ enabled: Bool) {
        strokeDedupe = enabled
        dk_set_stroke_dedupe(handle, enabled ? 1 : 0)
    }

    func setSelection(_ rect: SurfaceRect?) {
        if let rect {
            dk_set_selection(handle, UInt32(rect.x), UInt32(rect.y),
                             UInt32(rect.width), UInt32(rect.height))
            selection = rect
        } else {
            dk_clear_selection(handle)
            selection = nil
        }
        revision &+= 1
    }

    func selectAll() {
        setSelection(SurfaceRect(x: 0, y: 0, width: width, height: height))
    }

    func clearSelection() {
        setSelection(nil)
    }

    @discardableResult
    func extractSelection() -> Bool {
        guard dk_extract_selection(handle) != 0 else { return false }
        edited()
        syncLayers()
        redrawAll()
        scheduleThumbnails()
        return true
    }

    func setAutoShape(_ enabled: Bool) {
        autoShapeEnabled = enabled
        dk_set_auto_shape(handle, enabled ? 1 : 0)
    }

    func setPrediction(_ enabled: Bool) {
        prediction = enabled
        dk_set_prediction(handle, enabled ? 1 : 0)
    }

    func setQuickShape(_ enabled: Bool) {
        quickShape = enabled
        dk_set_quick_shape(handle, enabled ? 1 : 0)
    }

    func setBrush(_ kind: BrushKind) {
        brushKind = kind
        dk_set_brush(handle, Int32(kind.rawValue))
        refreshBrush()
    }

    func setMirror(enabled: Bool, angle: Double, both: Bool) {
        mirrorEnabled = enabled
        mirrorAngle = angle
        mirrorBoth = both
        applyMirror()
    }

    func applyMirror() {
        if mirrorEnabled {
            dk_set_mirror_angle(handle, Float(mirrorAngle), mirrorBoth ? 1 : 0)
        } else {
            dk_set_mirror(handle, 0)
        }
    }

    func setStabilization(_ strength: Double) {
        dk_set_stabilization(handle, Float(strength))
    }

    /// Close any stroke a lost pointer-up left open, then start the new one. Every segment until
    /// `endStroke` becomes one undo step and one recording group.
    func setDeferredRaster(_ enabled: Bool) {
        guard handle != nil else { return }
        dk_set_defer_raster(handle, enabled ? 1 : 0)
    }

    func beginStroke() {
        stopPlayback()
        finishFillAnimation()
        dk_stroke_end(handle)
        dk_stroke_begin(handle, now)
    }

    func endStroke(quickShape held: Bool = false) {
        dk_stroke_end(handle)
        if (autoShapeEnabled || (held && quickShape)), dk_auto_shape(handle) != 0 {
            edited()
            syncLayers()
            refresh()
        }
        scheduleThumbnails()
        if frameCount > 1 {
            refreshActiveFrameThumbnail()
        }
        revision &+= 1
    }

    func stroke(from: CGPoint, to: CGPoint, radius: CGFloat, pressure: CGFloat,
                tilt: CGPoint = .zero, azimuth: CGFloat = 0, roll: CGFloat = 0, ink: Ink) {
        dk_stroke_ex(handle, Float(from.x), Float(from.y), Float(to.x), Float(to.y), Float(radius),
                     Float(pressure), Float(tilt.x), Float(tilt.y), Float(azimuth), Float(roll),
                     ink.red, ink.green, ink.blue, ink.alpha, now)
        isBlank = false
        edited()
        refresh(notify: false)
    }

    func smudge(from: CGPoint, to: CGPoint, radius: CGFloat, pressure: CGFloat, strength: Double) {
        dk_smudge_ex(handle, Float(from.x), Float(from.y), Float(to.x), Float(to.y),
                     Float(radius), Float(pressure), Float(strength), now)
        edited()
        refresh(notify: false)
    }

    func erase(from: CGPoint, to: CGPoint, radius: CGFloat, pressure: CGFloat,
               tilt: CGPoint = .zero, azimuth: CGFloat = 0, roll: CGFloat = 0) {
        dk_erase_ex(handle, Float(from.x), Float(from.y), Float(to.x), Float(to.y), Float(radius),
                    Float(pressure), Float(tilt.x), Float(tilt.y), Float(azimuth), Float(roll), now)
        edited()
        refresh(notify: false)
    }

    /// Start a Procreate-style fill: the core prepares the mask, then this task reveals it from
    /// the tap outward. The final fill is one undo step and one tape event.
    @discardableResult
    func beginFill(at point: CGPoint, tolerance: Float, close: Int, expand: Int, feather: Int,
                   ink: Ink) -> Bool {
        finishFillAnimation()
        guard dk_fill_begin(handle, UInt32(point.x), UInt32(point.y), tolerance,
                            UInt32(close), UInt32(expand), UInt32(feather),
                            ink.red, ink.green, ink.blue, ink.alpha, now) != 0 else {
            return false
        }
        isBlank = false
        fillTask = Task { @MainActor [weak self] in
            guard let self else { return }
            let start = Date()
            let duration = 0.22
            while !Task.isCancelled {
                let progress = min(Date().timeIntervalSince(start) / duration, 1.0)
                let eased = 1.0 - pow(1.0 - progress, 3.0)
                _ = dk_fill_step(self.handle, Float(eased))
                self.refresh()
                if progress >= 1.0 { break }
                try? await Task.sleep(for: .milliseconds(16))
            }
            guard !Task.isCancelled else { return }
            _ = dk_fill_finish(self.handle)
            self.fillTask = nil
            self.edited()
            self.refresh()
            self.refreshThumbnails()
        }
        return true
    }

    /// Finish a fill that was still spreading, so a stroke or undo sees a complete step.
    func finishFillAnimation() {
        guard fillTask != nil else { return }
        fillTask?.cancel()
        fillTask = nil
        _ = dk_fill_finish(handle)
        edited()
        refresh()
        refreshThumbnails()
    }

    func cancelFillAnimation() {
        guard fillTask != nil else { return }
        fillTask?.cancel()
        fillTask = nil
        _ = dk_fill_cancel(handle)
        refresh()
    }

    func fill(at point: CGPoint, tolerance: Float,
              close: Int, expand: Int, feather: Int, ink: Ink) {
        dk_fill_ex(handle, UInt32(point.x), UInt32(point.y), tolerance,
                   UInt32(close), UInt32(expand), UInt32(feather),
                   ink.red, ink.green, ink.blue, ink.alpha, now)
        isBlank = false
        edited()
        refresh()
        refreshThumbnails()
    }

    /// Extract the visible painting into a new line layer with the embedded TEED model.
    func extractLineArt() {
        guard dk_extract_lineart(handle, 0.55, 0.12) != 0 else {
            NSSound.beep()
            flash("No line art found", error: true)
            return
        }
        edited()
        syncLayers()
        redrawAll()
        scheduleThumbnails()
        flash("Line art layer added")
    }

    /// The dominant colours in the painting, for the picker's "from painting" row.
    func palette(count: Int = 6) -> [Color] {
        var values = [Float](repeating: 0, count: count * 4)
        let written = values.withUnsafeMutableBufferPointer {
            dk_palette(handle, UInt32(count), $0.baseAddress)
        }
        guard written > 0 else { return [] }
        return (0..<Int(written)).map { index in
            Color(.sRGB,
                  red: Double(values[index * 4]),
                  green: Double(values[index * 4 + 1]),
                  blue: Double(values[index * 4 + 2]),
                  opacity: Double(values[index * 4 + 3]))
        }
    }

    /// The colour the painter sees at this pixel. Returns nil outside the canvas.
    func pick(at point: CGPoint) -> Color? {
        var rgba = [Float](repeating: 0, count: 4)
        let found = rgba.withUnsafeMutableBufferPointer {
            dk_pick(handle, UInt32(point.x), UInt32(point.y), $0.baseAddress)
        }
        guard found != 0 else { return nil }
        return Color(.sRGB, red: Double(rgba[0]), green: Double(rgba[1]),
                     blue: Double(rgba[2]), opacity: Double(rgba[3]))
    }

    /// Pick a colour and, on an HDR document, the stops above white that would reproduce it.
    /// The returned colour is normalised to 1.0 at its brightest channel, so the Glow control can
    /// put the highlight back without a colour that SwiftUI would clamp.
    func pickInk(at point: CGPoint) -> (Color, Double)? {
        var rgba = [Float](repeating: 0, count: 4)
        let found = rgba.withUnsafeMutableBufferPointer {
            dk_pick(handle, UInt32(point.x), UInt32(point.y), $0.baseAddress)
        }
        guard found != 0 else { return nil }
        let peak = max(rgba[0], max(rgba[1], rgba[2]))
        let glow = (hdr && peak > 1) ? min(log2(Double(peak)), 4) : 0
        let scale = glow > 0 ? 1 / Double(peak) : 1
        let color = Color(.sRGB, red: Double(rgba[0]) * scale,
                          green: Double(rgba[1]) * scale,
                          blue: Double(rgba[2]) * scale,
                          opacity: Double(rgba[3]))
        return (color, glow)
    }

    /// Undo walks one timeline, so a layer was added or a blend mode changed by the same stack as
    /// a stroke. The shell has to rebuild its layer list as well as the pixels: an undone add
    /// removes a row, an undone opacity restores a value the palette is showing.
    func undo() {
        finishFillAnimation()
        guard dk_undo(handle) != 0 else { return }
        edited()
        syncLayers()
        redrawAll()
        scheduleThumbnails()
    }

    func redo() {
        finishFillAnimation()
        guard dk_redo(handle) != 0 else { return }
        edited()
        syncLayers()
        redrawAll()
        scheduleThumbnails()
    }
    func clear() { dk_clear(handle, 1, 1, 1, 0); edited(); refresh(); scheduleThumbnails() }
}
