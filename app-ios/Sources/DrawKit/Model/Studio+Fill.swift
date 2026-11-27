// The spreading fill: the core prepares the mask, this task reveals it from the tap outwards.

import CoreGraphics
import Foundation

@MainActor
extension Studio {
    @discardableResult
    func beginFill(at point: CGPoint) -> Bool {
        // The mask is decided from the core cache, so a previous GPU commit must reach it first.
        materializeCanvas()
        finishFillAnimation()
        stampTime()
        let components = colorComponents()
        guard
            dk_fill_begin(
                handle, UInt32(max(0, point.x)), UInt32(max(0, point.y)),
                Float(tolerance), UInt32(max(0, fillClose)),
                UInt32(max(0, fillExpand)), UInt32(max(0, fillFeather)),
                components.0, components.1, components.2,
                Float(opacity), lastTime) != 0
        else {
            return false
        }
        isBlank = false
        fillTask = Task { @MainActor [weak self] in
            guard let self else { return }
            let start = Date()
            while !Task.isCancelled {
                let progress = min(Date().timeIntervalSince(start) / 0.22, 1.0)
                let eased = 1.0 - pow(1.0 - progress, 3.0)
                _ = dk_fill_step(self.handle, Float(eased))
                _ = self.refreshDirty()
                if progress >= 1.0 { break }
                try? await Task.sleep(for: .milliseconds(16))
            }
            guard !Task.isCancelled else { return }
            _ = dk_fill_finish(self.handle)
            self.fillTask = nil
            self.rememberColour()
            self.refreshAll()
            self.refreshLayers()
            self.finishChange()
        }
        return true
    }

    func finishFillAnimation() {
        guard fillTask != nil else { return }
        fillTask?.cancel()
        fillTask = nil
        _ = dk_fill_finish(handle)
        rememberColour()
        refreshAll()
        refreshLayers()
        finishChange()
    }
}
