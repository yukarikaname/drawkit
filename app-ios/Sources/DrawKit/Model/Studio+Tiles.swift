import SwiftUI

extension Studio {
    /// Begin the surface-side preview. The caller sets the deferred flag before `beginStroke`:
    /// the core captures it when the session starts, and only then does the preview replace the
    /// CPU raster instead of duplicating it.
    func beginGPUStroke(_ surface: PaintSurface) {
        let kind = Int32(dk_brush_param(handle, 14))
        var seedHalves = [UInt32](repeating: 0, count: 2)
        _ = seedHalves.withUnsafeMutableBufferPointer { dk_stroke_seed(handle, $0.baseAddress) }
        let seed = UInt64(seedHalves[0]) | (UInt64(seedHalves[1]) << 32)
        var size = [UInt32](repeating: 0, count: 2)
        if size.withUnsafeMutableBufferPointer({ dk_brush_texture_size(handle, $0.baseAddress) }) != 0,
           size[0] > 0, size[1] > 0 {
            var data = [UInt8](repeating: 0, count: Int(size[0]) * Int(size[1]))
            if data.withUnsafeMutableBufferPointer({
                dk_brush_texture_preview(handle, size[0], size[1], $0.baseAddress)
            }) != 0 {
                surface.setBrushMap(width: Int(size[0]), height: Int(size[1]), data: data)
            }
        } else {
            surface.clearBrushMap()
        }
        let ink = surface.extended ? linearInk(colorComponents()) : colorComponents()
        surface.strokeBegin(width: width, height: height,
            color: (ink.0, ink.1, ink.2, 1),
            flow: dk_brush_param(handle, 1), pressureAlpha: dk_brush_param(handle, 3),
            paper: dk_brush_param(handle, 9), tip: dk_brush_param(handle, 10),
            dual: dk_brush_param(handle, 12), dualScale: dk_brush_param(handle, 13),
            dualHardness: dk_brush_kind_param(kind, 0),
            dualRoundness: dk_brush_kind_param(kind, 7),
            dualAngle: dk_brush_kind_param(kind, 6),
            dualPaper: dk_brush_kind_param(kind, 9),
            dualTip: dk_brush_kind_param(kind, 10),
            wet: dk_brush_param(handle, 11),
            jitter: dk_brush_param(handle, 5), grain: dk_brush_param(handle, 8),
            dualJitter: dk_brush_kind_param(kind, 5),
            dualGrain: dk_brush_kind_param(kind, 8),
            seed: seed)
    }

    func beginTiles() -> Bool {
        guard dk_stroke_tile_begin(handle) != 0 else { return false }
        deferredPending = false
        return true
    }

    @discardableResult
    func endGPUTiles(rects: [UInt32]) -> Bool {
        let applied = rects.withUnsafeBufferPointer {
            dk_stroke_gpu_end(handle, $0.baseAddress, UInt32($0.count / 4))
        }
        if applied != 0 {
            gpuStale = true
            revision &+= 1
        }
        return applied != 0
    }

    /// Rebuild the CPU cache from the resident tiles before anything reads it.
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
}

extension Studio {
    func undo() {
        if dk_undo_is_gpu(handle) != 0 {
            guard dk_undo(handle) != 0 else { return }
            _ = paintSurface?.undoTiles()
            gpuStale = true
            refreshLayers()
            finishChange()
            return
        }
        materializeCanvas()
        guard dk_undo(handle) != 0 else { return }
        refreshAll()
        refreshLayers()
        finishChange()
    }

    func redo() {
        if dk_redo_is_gpu(handle) != 0 {
            guard dk_redo(handle) != 0 else { return }
            _ = paintSurface?.redoTiles()
            gpuStale = true
            refreshLayers()
            finishChange()
            return
        }
        materializeCanvas()
        guard dk_redo(handle) != 0 else { return }
        refreshAll()
        refreshLayers()
        finishChange()
    }

    /// Start a tile commit. False means the core refuses this brush and the shell replays instead.
}
