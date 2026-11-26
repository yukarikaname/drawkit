// Brush, fill, smudge and the GPU canvas bookkeeping.

import SwiftUI
import AppKit

extension PaintModel {
    // MARK: Painting

    var stride: Int { width * 4 }

    func edited() {
        stopPlayback()
        autosaveBlockedUntilEdit = false
        if !isDirty {
            isDirty = true
        }
        editCount &+= 1
        refreshHistoryBranches()
    }

    /// Adobe RGB has no eight-bit drawable, so it presents through the linear path like HDR.
    var needsLinear: Bool { hdr || colorSpace == .adobeRgb }

    /// Milliseconds since this painting began. The tape is the document, so every stroke carries a
    /// timestamp even when nobody asked for a replay; otherwise all the events would sit at zero
    /// and a timelapse would have nothing to order.
    var now: UInt64 {
        UInt64(max(0, Date().timeIntervalSince(started)) * 1000)
    }

    /// Hand the live pixels to a GPU surface. The buffer is the same one the core composites into,
    /// so presenting does not copy or re-encode it.
    func withPixels(_ body: (UnsafeMutablePointer<UInt8>, Int, Int) -> Void) {
        body(pixels.pointer, width, height)
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

    func endStroke(quickShape held: Bool = false, snap: Bool = false) {
        dk_stroke_end(handle)
        deferredPending = dk_deferred_pending(handle) != 0
        if (autoShapeEnabled || (held && quickShape)) {
            flushDeferred()
            let shaped = snap ? dk_auto_shape_snapped(handle) : dk_auto_shape(handle)
            if shaped != 0 {
                edited()
                syncLayers()
                refresh()
            }
        }
        scheduleThumbnails()
        if frameCount > 1 {
            refreshActiveFrameThumbnail()
        }
        revision &+= 1
    }

    func advanceDeferred(commands: UInt32 = 24) -> Bool {
        guard deferredPending else { return true }
        let done = dk_deferred_step(handle, commands) != 0
        deferredPending = !done
        return done
    }

    /// The seed of the stroke in progress, for the GPU jitter and grain.
    func currentStrokeSeed() -> UInt64 {
        var halves = [UInt32](repeating: 0, count: 2)
        guard halves.withUnsafeMutableBufferPointer({
            dk_stroke_seed(handle, $0.baseAddress)
        }) != 0 else { return 0 }
        return UInt64(halves[0]) | (UInt64(halves[1]) << 32)
    }

    /// Start a tile commit. False means the core refuses this brush, so the shell keeps the
    /// sliced replay.
    func beginTiles() -> Bool {
        guard dk_stroke_tile_begin(handle) != 0 else { return false }
        deferredPending = false
        return true
    }

    /// Close a GPU commit. The pixels stay in the resident tiles, so the history only records the
    /// rectangles and the CPU cache is marked stale.
    @discardableResult
    func endGPUTiles(rects: [UInt32]) -> Bool {
        let applied = rects.withUnsafeBufferPointer {
            dk_stroke_gpu_end(handle, $0.baseAddress, UInt32($0.count / 4))
        }
        if applied != 0 {
            gpuStale = true
            scheduleThumbnails()
            if frameCount > 1 {
                refreshActiveFrameThumbnail()
            }
            revision &+= 1
        }
        return applied != 0
    }

    /// Bring the CPU cache up to date with the resident tiles before anything reads or writes it.
    func materializeCanvas() {
        guard gpuStale, let surface = paintSurface else { return }
        if surface.extended {
            guard surface.materializeLinear() else { return }
            var index = 0
            while let tile = surface.linearTile(index) {
                _ = tile.pixels.withUnsafeBufferPointer { buffer in
                    dk_stroke_apply_tiles_linear(handle, UInt32(tile.layer),
                                                 UInt32(tile.x), UInt32(tile.y),
                                                 UInt32(tile.width), UInt32(tile.height),
                                                 buffer.baseAddress)
                }
                index += 1
            }
            surface.clearLinearCommitted()
            gpuStale = false
            return
        }
        guard surface.materialize() else { return }
        var index = 0
        while let tile = surface.committedTile(index) {
            let layer = surface.committedLayer(index)
            if layer >= 0 {
                _ = tile.pixels.withUnsafeBufferPointer { buffer in
                    dk_stroke_apply_tiles(handle, UInt32(layer), UInt32(tile.x), UInt32(tile.y),
                                          UInt32(tile.width), UInt32(tile.height),
                                          buffer.baseAddress)
                }
            }
            index += 1
        }
        surface.clearCommitted()
        gpuStale = false
    }

    func undoIsGPU() -> Bool { dk_undo_is_gpu(handle) != 0 }
    func redoIsGPU() -> Bool { dk_redo_is_gpu(handle) != 0 }

    func flushDeferred() {
        materializeCanvas()
        guard deferredPending else { return }
        dk_flush_deferred(handle)
        deferredPending = false
        scheduleThumbnails()
        revision &+= 1
    }

    /// `update` is false for a stroke the GPU canvas is previewing: the core still records the
    /// segment for the tape, but refreshing would materialise the previous commit into the CPU
    /// surface on every pointer sample, which doubles the document's pixels.
    func stroke(from: CGPoint, to: CGPoint, radius: CGFloat, pressure: CGFloat,
                tilt: CGPoint = .zero, azimuth: CGFloat = 0, roll: CGFloat = 0, ink: Ink,
                update: Bool = true) {
        dk_stroke_ex(handle, Float(from.x), Float(from.y), Float(to.x), Float(to.y), Float(radius),
                     Float(pressure), Float(tilt.x), Float(tilt.y), Float(azimuth), Float(roll),
                     ink.red, ink.green, ink.blue, ink.alpha, now)
        isBlank = false
        edited()
        if update {
            refresh(notify: false)
        }
    }

    /// The mirrored copies of one segment, four floats each: from x, from y, to x, to y. Mirroring
    /// lives in the core, so this asks the editor handle, never the display surface.
    func strokeVariants(fromX: Float, fromY: Float, toX: Float, toY: Float,
                        _ body: (_ fromX: Float, _ fromY: Float,
                                 _ toX: Float, _ toY: Float) -> Void) {
        var buffer = [Float](repeating: 0, count: 16)
        let count = buffer.withUnsafeMutableBufferPointer {
            dk_mirror_segments(handle, fromX, fromY, toX, toY, $0.baseAddress)
        }
        guard count > 0 else {
            body(fromX, fromY, toX, toY)
            return
        }
        for index in 0..<Int(count) {
            body(buffer[index * 4], buffer[index * 4 + 1],
                 buffer[index * 4 + 2], buffer[index * 4 + 3])
        }
    }

    func smudge(from: CGPoint, to: CGPoint, radius: CGFloat, pressure: CGFloat,
                strength: Double, update: Bool = true) {
        dk_smudge_ex(handle, Float(from.x), Float(from.y), Float(to.x), Float(to.y),
                     Float(radius), Float(pressure), Float(strength), now)
        edited()
        if update {
            refresh(notify: false)
        }
    }

    func erase(from: CGPoint, to: CGPoint, radius: CGFloat, pressure: CGFloat,
               tilt: CGPoint = .zero, azimuth: CGFloat = 0, roll: CGFloat = 0,
               update: Bool = true) {
        dk_erase_ex(handle, Float(from.x), Float(from.y), Float(to.x), Float(to.y), Float(radius),
                    Float(pressure), Float(tilt.x), Float(tilt.y), Float(azimuth), Float(roll), now)
        edited()
        if update {
            refresh(notify: false)
        }
    }

    /// Extract the visible painting into a new line layer with the embedded TEED model.
    func extractLineArt() {
        guard dk_extract_lineart(handle, 0.55, 0.12) != 0 else {
            NSSound.beep()
            flash(String(localized: "No line art found"), error: true)
            return
        }
        edited()
        syncLayers()
        redrawAll()
        scheduleThumbnails()
        flash(String(localized: "Line art layer added"))
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

    /// The colour the painter sees at this pixel. Returns nil outside the canvas. The surface is
    /// the only source: an extended surface hands back linear Display P3, which is converted to
    /// the working space before it becomes a colour.
    func pick(at point: CGPoint) -> Color? {
        guard let hit = paintSurface?.pick(x: Int(point.x), y: Int(point.y)) else {
            return nil
        }
        let values = workingColour(hit)
        return Color(.sRGB, red: Double(values.0), green: Double(values.1),
                     blue: Double(values.2), opacity: 1)
    }

    /// Convert an extended-surface pick (premultiplied linear Display P3 over paper) into the
    /// document's encoded working space.
    private func workingColour(_ pick: (Float, Float, Float, Float))
        -> (Float, Float, Float, Float) {
        guard paintSurface?.extended == true else { return pick }
        var values = [pick.0, pick.1, pick.2]
        let converted = values.withUnsafeMutableBufferPointer {
            dk_working_from_display_p3(handle, pick.0, pick.1, pick.2, $0.baseAddress)
        }
        guard converted != 0 else { return pick }
        return (values[0], values[1], values[2], pick.3)
    }

    /// Pick a colour and, on an HDR document, the stops above white that would reproduce it.
    /// The returned colour is normalised to 1.0 at its brightest channel, so the Glow control can
    /// put the highlight back without a colour that SwiftUI would clamp.
    func pickInk(at point: CGPoint) -> (Color, Double)? {
        guard let hit = paintSurface?.pick(x: Int(point.x), y: Int(point.y)) else {
            return nil
        }
        let values = workingColour(hit)
        let peak = max(values.0, max(values.1, values.2))
        let glow = (hdr && peak > 1) ? min(log2(Double(peak)), 4) : 0
        let scale = glow > 0 ? 1 / Double(peak) : 1
        let color = Color(.sRGB, red: Double(values.0) * scale,
                          green: Double(values.1) * scale,
                          blue: Double(values.2) * scale,
                          opacity: Double(values.3))
        return (color, glow)
    }

    /// Undo walks one timeline, so a layer was added or a blend mode changed by the same stack as
    /// a stroke. The shell has to rebuild its layer list as well as the pixels: an undone add
    /// removes a row, an undone opacity restores a value the palette is showing.
    func undo() {
        finishFillAnimation()
        if undoIsGPU() {
            guard dk_undo(handle) != 0 else { return }
            _ = paintSurface?.undoTiles()
            gpuStale = true
            edited()
            syncLayers()
            revision &+= 1
            scheduleThumbnails()
            return
        }
        materializeCanvas()
        guard dk_undo(handle) != 0 else { return }
        edited()
        syncLayers()
        redrawAll()
        scheduleThumbnails()
    }

    func redo() {
        finishFillAnimation()
        if redoIsGPU() {
            guard dk_redo(handle) != 0 else { return }
            _ = paintSurface?.redoTiles()
            gpuStale = true
            edited()
            syncLayers()
            revision &+= 1
            scheduleThumbnails()
            return
        }
        materializeCanvas()
        guard dk_redo(handle) != 0 else { return }
        edited()
        syncLayers()
        redrawAll()
        scheduleThumbnails()
    }
    func clear() { dk_clear(handle, 1, 1, 1, 0); edited(); refresh(); scheduleThumbnails() }
}
