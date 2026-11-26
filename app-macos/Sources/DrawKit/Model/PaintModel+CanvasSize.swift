// Changing the canvas size. The core moves every layer and frame; the shell only has to adopt the
// new geometry and upload the whole stack again.

import Foundation

extension PaintModel {
    /// Crop or pad the canvas, keeping the art at `anchorX`/`anchorY` (0 left/top, 1 right/bottom).
    /// The core starts a new history baseline, so the resize itself is not undoable.
    @discardableResult
    func resizeCanvas(to width: Int, height: Int, anchorX: Double, anchorY: Double) -> Bool {
        flushDeferred()
        let offsetX = Int((Double(width - self.width) * anchorX).rounded())
        let offsetY = Int((Double(height - self.height) * anchorY).rounded())
        guard dk_resize_canvas(handle, UInt32(width), UInt32(height),
                               Int32(offsetX), Int32(offsetY)) != 0 else { return false }
        adoptCanvasSize(width: width, height: height)
        selection = nil
        paintSurface?.clearTileHistory()
        gpuStale = false
        markGPURebuild()
        markSurfaceFull()
        edited()
        refresh()
        syncLayers()
        refreshThumbnails()
        refreshFrames()
        revision &+= 1
        requestPresent?()
        return true
    }
}
