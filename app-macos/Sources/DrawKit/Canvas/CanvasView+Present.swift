// Coalesced presentation: pointer samples mark the canvas dirty and one main-queue pass presents
// the newest state, so a burst of samples does not queue a present each.

import Foundation

extension CanvasView {
    /// Ask for a present after the current event is handled. Several samples collapse into one.
    func schedulePresent() {
        guard !presentPending else { return }
        presentPending = true
        DispatchQueue.main.async { [weak self] in
            guard let self else { return }
            self.presentPending = false
            self.presentSurface()
        }
    }
}
