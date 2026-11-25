// A finished stroke can be committed straight into the resident layer tiles: the GPU composites
// the coverage there, and only the touched tiles are read back for history. The preview comes down
// as soon as the tiles hold the result, so the readback never blocks the pen.

import AppKit
import Foundation

extension CanvasView {
    /// Commit the live stroke into the resident layer. False means the brush or the canvas is not
    /// one the preview reproduces, so the caller keeps the sliced replay.
    func startTileCommit() -> Bool {
        guard gpuStroke, let model, let surface, model.beginTiles() else { return false }
        let count = surface.strokeCommit(
            layer: model.activeLayer,
            color: (ink.red, ink.green, ink.blue),
            flow: Float(model.brushParameters[1]))
        guard count > 0 else {
            model.endTiles()
            return false
        }
        // The resident tiles now hold the final pixels: drop the coverage overlay and show them
        // without uploading the (still stale) CPU composite over the top.
        surface.strokeEnd()
        model.setDeferredRaster(false)
        gpuStroke = false
        presentSurface(force: true, upload: false)
        startTileReadback()
        return true
    }

    /// Finish a readback that is still in flight, for the next stroke or an undo.
    func flushTileCommit() {
        guard tileTimer != nil, let model, let surface else { return }
        tileTimer?.invalidate()
        tileTimer = nil
        if model.finishTiles(surface) {
            model.markSurfaceFull()
        }
    }

    private func startTileReadback() {
        tileTimer?.invalidate()
        let timer = Timer(timeInterval: 1.0 / 120.0, repeats: true) { [weak self] _ in
            guard let self, let model = self.model, let surface = self.surface else { return }
            guard surface.commitReady() else { return }
            self.tileTimer?.invalidate()
            self.tileTimer = nil
            if model.finishTiles(surface) {
                model.markSurfaceFull()
            }
            self.overlay.needsDisplay = true
        }
        RunLoop.main.add(timer, forMode: .common)
        tileTimer = timer
    }
}
