// Gesture and Pencil interaction conformances for the iPad canvas. The delegate reports the
// gesture; the preferred action is read here because it is a class property on the interaction,
// and the shell maps it to a tool so the menu bar and the Pencil do the same thing.

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
        onPencilTap?(UIPencilInteraction.preferredTapAction)
    }

    /// Pencil Pro squeeze. Only the end of the gesture counts; began/changed would toggle twice.
    func pencilInteraction(_ interaction: UIPencilInteraction,
                           didReceiveSqueeze squeeze: UIPencilInteraction.Squeeze) {
        guard squeeze.phase == .ended else { return }
        onPencilSqueeze?(UIPencilInteraction.preferredSqueezeAction)
    }
}
