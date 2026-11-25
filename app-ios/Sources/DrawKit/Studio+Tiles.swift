import SwiftUI

extension Studio {
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
        guard gpuStale, let surface = paintSurface, surface.materialize() else { return }
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
