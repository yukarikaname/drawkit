// Picking a recorded stroke and editing it. The edits live on the model because they cross the
// core, the resident layers and the undo history in one step.

import SwiftUI

extension PaintModel {
    /// The recorded stroke under a document point, or 0 when nothing is there.
    func strokeGroup(at point: CGPoint) -> UInt64 {
        dk_stroke_at(handle, Float(point.x), Float(point.y))
    }

    /// The document rectangle a recorded stroke can reach.
    func strokeBounds(_ group: UInt64) -> SurfaceRect? {
        guard group != 0 else { return nil }
        var rect = [UInt32](repeating: 0, count: 4)
        let found = rect.withUnsafeMutableBufferPointer {
            dk_stroke_bounds(handle, group, $0.baseAddress)
        }
        guard found != 0 else { return nil }
        return SurfaceRect(x: Int(rect[0]), y: Int(rect[1]),
                           width: Int(rect[2]), height: Int(rect[3]))
    }

    /// Re-render one recorded stroke in `ink`. The core rebuilds the layer from the tape, so the
    /// anti-aliased edge changes with the stroke instead of keeping the old colour.
    @discardableResult
    func recolorStroke(_ group: UInt64, ink: Ink) -> Bool {
        flushDeferred()
        materializeCanvas()
        guard group != 0,
              dk_recolor_stroke(handle, group, ink.red, ink.green, ink.blue, ink.alpha) != 0
        else { return false }
        strokeEditFinished()
        return true
    }

    /// Remove one recorded stroke. False when the layer holds pixels the tape cannot rebuild.
    @discardableResult
    func deleteStroke(_ group: UInt64) -> Bool {
        flushDeferred()
        materializeCanvas()
        guard group != 0, dk_delete_stroke(handle, group) != 0 else { return false }
        strokeEditFinished()
        return true
    }

    /// The edit rebuilt the CPU layer; the resident tiles and the tile journal describe the old
    /// one, so they are dropped and the whole stack is uploaded again.
    private func strokeEditFinished() {
        paintSurface?.clearTileHistory()
        gpuStale = false
        markGPURebuild()
        markSurfaceFull()
        edited()
        refresh()
        requestPresent?()
        refreshThumbnails()
    }

}
