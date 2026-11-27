// Presenting the resident layer stack on the iPad canvas.

import QuartzCore
import UIKit

extension SketchCanvasView {
    /// Upload what the studio still needs and composite the resident layers on the GPU. The CPU
    /// composite is only rebuilt by an export, a thumbnail or a pixel tool.
    func presentSurface() {
        guard bounds.width > 0, bounds.height > 0 else {
            requestOverlayRedraw()
            return
        }
        updateSurface()
        guard let presenter else {
            setNeedsDisplay()
            requestOverlayRedraw()
            return
        }
        let rect = paperRect
        guard rect.width > 0, rect.height > 0 else { return }
        let scale = max(contentScaleFactor, 1)
        let paper = CGRect(
            x: rect.minX * scale,
            y: (bounds.height - rect.maxY) * scale,
            width: rect.width * scale,
            height: rect.height * scale)
        presenter.present(paper: paper)
        requestOverlayRedraw()
    }

    /// Give the surface a presenter for this document. A new drawable starts with an empty
    /// resident stack.
    func installPresenter(for surface: PaintSurface, studio: Studio) {
        let presenter = CanvasPresenter(surface: surface, source: studio)
        presenter.afterPresent = { [weak self] _ in
            // A frame presented outside a SwiftUI update still has to reach the window server.
            CATransaction.flush()
            self?.requestOverlayRedraw()
        }
        self.presenter = presenter
    }
}
