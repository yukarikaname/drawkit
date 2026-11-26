// The timeline. One replay compositor lives on the canvas surface's device, so a scrubbed frame
// is presented on the GPU instead of being read back and uploaded.

import CoreGraphics
import Foundation

extension PaintModel {
    /// Advance the replay to `ms`, reusing the compositor across the scrub.
    func refreshPreview(at ms: UInt64) {
        if tapeReplay == nil && !tapeReplayUnavailable, let handle {
            if let surface = paintSurface, surface.supportsLayers,
               let compositor = surface.makeReplay(width: width, height: height) {
                tapeReplay = TapeReplay(handle: handle, width: width, height: height,
                                        compositor: compositor, surface: surface)
            } else {
                tapeReplay = TapeReplay(handle: handle, width: width, height: height)
            }
            tapeReplayUnavailable = tapeReplay == nil
        }
        _ = tapeReplay?.apply(to: ms)
    }

    /// Present the current timeline frame on the surface. Called by the view, which owns the paper
    /// rectangle.
    func presentPreview(surface: PaintSurface, paper: CGRect) -> Bool {
        tapeReplay?.present(paper: paper) ?? false
    }
}
