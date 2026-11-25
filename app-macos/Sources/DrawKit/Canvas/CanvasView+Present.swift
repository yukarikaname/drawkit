// Frame-paced presentation: pointer samples mark the canvas dirty and the display link presents
// the newest state once per refresh.

import AppKit
import QuartzCore

extension CanvasView {
    func startPresentLoop() {
        guard frameLink == nil else { return }
        let link = displayLink(target: self, selector: #selector(frameTick(_:)))
        link.add(to: .main, forMode: .common)
        frameLink = link
    }

    func stopPresentLoop() {
        frameLink?.invalidate()
        frameLink = nil
        presentPending = false
    }

    /// Ask for a present on the next display refresh. Several samples can collapse into one.
    func schedulePresent() {
        presentPending = true
    }

    @objc private func frameTick(_ link: CADisplayLink) {
        guard presentPending else { return }
        presentPending = false
        presentSurface()
    }
}
