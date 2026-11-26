// Gesture and Pencil interaction conformances for the iPad canvas.

import UIKit

extension SketchCanvasView: UIGestureRecognizerDelegate {
    func gestureRecognizer(_ gestureRecognizer: UIGestureRecognizer,
                           shouldRecognizeSimultaneouslyWith other: UIGestureRecognizer) -> Bool {
        true
    }
}

extension SketchCanvasView: UIPencilInteractionDelegate {
    func pencilInteraction(_ interaction: UIPencilInteraction,
                           didReceiveTap tap: UIPencilInteraction.Tap) {
        onPencilDoubleTap?()
    }

    /// Pencil Pro squeeze is the same intent as a double tap: cycle the eraser.
    func pencilInteraction(_ interaction: UIPencilInteraction,
                           didReceiveSqueeze squeeze: UIPencilInteraction.Squeeze) {
        if squeeze.phase == .ended { onPencilDoubleTap?() }
    }
}
