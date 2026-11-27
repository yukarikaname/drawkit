// The deferred-commit hand-off: a GPU stroke becomes a tile commit once the tape has caught up.

import AppKit

extension CanvasView {
    func startGPUCommit() {
        guard gpuStroke, let model else {
            finishGPUStroke()
            return
        }
        if !model.deferredPending {
            finishGPUStroke()
            return
        }
        commitTask?.cancel()
        commitTask = Task { @MainActor [weak self] in
            while !Task.isCancelled {
                guard let self, let model = self.model else { return }
                if model.advanceDeferred(commands: 8) {
                    self.finishGPUStroke()
                    return
                }
                try? await Task.sleep(for: .milliseconds(16))
            }
        }
    }

    func finishGPUStroke() {
        guard gpuStroke else { return }
        gpuStroke = false
        commitTask?.cancel()
        commitTask = nil
        model?.setDeferredRaster(false)
        surface?.strokeEnd()
        // The alpha lock belongs to one stroke; a quick shape or a refused tile commit ends the
        // stroke without the commit that would have consumed it.
        surface?.setStrokeAlphaLock(false)
        model?.markGPURebuild()
        finishStrokePresent(upload: true)
    }

    /// Delete or recolour the picked stroke. A layer with an imported base or an effect the tape
    /// cannot rebuild refuses, and the canvas says so instead of changing pixels it did not paint.
    func applyStrokeEdit(delete: Bool, group: UInt64) {
        guard let model else { return }
        let changed = delete ? model.deleteStroke(group) : model.recolorStroke(group, ink: ink)
        guard changed else {
            NSSound.beep()
            model.flash(
                String(localized: "This layer has pixels the strokes did not paint"), error: true)
            return
        }
        selectedStroke = nil
        overlay.needsDisplay = true
        model.flash(
            delete ? String(localized: "Stroke deleted") : String(localized: "Stroke recoloured"))
    }
}
