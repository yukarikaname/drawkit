// A finished stroke is committed straight into the resident layer tiles. The pixels stay there:
// the replaced tiles are the undo record, and the CPU surface is only rebuilt by a save, an export
// or a pixel tool.

import AppKit
import Foundation

extension CanvasView {
    /// Commit the live stroke into the resident layer. False means the brush or the canvas is not
    /// one the preview reproduces, so the caller keeps the sliced replay.
    func startTileCommit() -> Bool {
        guard gpuStroke, let model, let surface else { return false }
        model.paintSurface = surface
        guard model.beginTiles() else { return false }
        surface.setStrokeAlphaLock(model.layers[model.activeLayer].alphaLocked)
        let count = surface.strokeCommit(
            layer: model.activeLayer,
            color: gpuInk,
            flow: Float(model.brushParameters[1]))
        guard count > 0 else {
            model.endGPUTiles(rects: [])
            return false
        }
        var rects: [UInt32] = []
        for index in 0..<count {
            guard let rect = surface.committedRect(index) else { continue }
            rects += [UInt32(rect.x), UInt32(rect.y), UInt32(rect.width), UInt32(rect.height)]
        }
        _ = model.endGPUTiles(rects: rects)
        // The resident tiles already hold the committed pixels: drop the overlay and show them
        // without uploading the (still stale) CPU composite over the top.
        surface.strokeEnd()
        model.setDeferredRaster(false)
        gpuStroke = false
        presentSurface(force: true, upload: true)
        return true
    }

    /// Close a smudge: the segments are already in the tiles, and the journal is one undo step.
    func finishGPUSmudge() {
        guard gpuSmudge, let model, let surface else {
            gpuSmudge = false
            gpuStroke = false
            return
        }
        gpuSmudge = false
        gpuStroke = false
        model.setDeferredRaster(false)
        let count = surface.endSmudgeJournal()
        var rects: [UInt32] = []
        for index in 0..<max(count, 0) {
            guard let rect = surface.committedRect(index) else { continue }
            rects += [UInt32(rect.x), UInt32(rect.y), UInt32(rect.width), UInt32(rect.height)]
        }
        if model.beginTiles() {
            _ = model.endGPUTiles(rects: rects)
        }
        surface.strokeEnd()
        presentSurface(force: true, upload: true)
    }
}
