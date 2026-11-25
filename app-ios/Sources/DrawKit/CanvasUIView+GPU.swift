// Extended-range preview and pencil dynamics kept out of the main canvas file.

import UIKit
import SwiftUI

extension SketchCanvasView {
    func startGPUCommit() {
        guard gpuStroke, let studio else {
            finishGPUStroke()
            return
        }
        if !studio.deferredPending {
            finishGPUStroke()
            return
        }
        commitTimer?.invalidate()
        let timer = Timer(timeInterval: 1.0 / 60.0, repeats: true) { [weak self] _ in
            guard let self, let studio = self.studio else { return }
            if studio.advanceDeferred(commands: 8) {
                self.commitTimer?.invalidate()
                self.commitTimer = nil
                self.finishGPUStroke()
            }
        }
        RunLoop.main.add(timer, forMode: .common)
        commitTimer = timer
    }

    func finishGPUStroke() {
        guard gpuStroke else { return }
        commitTimer?.invalidate()
        commitTimer = nil
        gpuStroke = false
        studio?.setDeferredRaster(false)
        surface?.strokeEnd()
        studio?.markSurfaceFull()
        presentSurface(force: true, upload: true)
    }

    /// Pressure is normalised force on a pencil; tilt adds a little width, the way a soft pencil
    /// spreads when it is laid down.
    func pencilDynamics(of touch: UITouch) -> (pressure: CGFloat, tilt: [Float],
                                               azimuth: Float, roll: Float) {
        guard touch.type == .pencil else {
            return (1, [0, 0], 0, 0)
        }
        let altitude = touch.altitudeAngle
        let lean = 1 - altitude / (.pi / 2)
        let azimuth = touch.azimuthAngle(in: self)
        let force = touch.force > 0 ? touch.force / max(touch.maximumPossibleForce, 1) : 0.6
        let pressure = min(1, force * (1.30 - 0.30 * altitude / (.pi / 2)))
        let tilt = [Float(cos(azimuth) * lean), Float(sin(azimuth) * lean)]
        return (pressure, tilt, Float(azimuth), Float(touch.rollAngle))
    }
}
