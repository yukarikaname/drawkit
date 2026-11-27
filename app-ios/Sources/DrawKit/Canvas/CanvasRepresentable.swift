// The SwiftUI host for the iPad canvas. The UIView stays alive across updates; a revision change
// only asks it to present the painting the studio already holds. The Pencil callbacks are passed
// through because the interaction lives in the UIView while the tool state lives in the studio.

import SwiftUI
import UIKit

struct CanvasView: UIViewRepresentable {
    let studio: Studio
    let revision: Int
    let onPencilTap: (UIPencilPreferredAction) -> Void
    let onPencilSqueeze: (UIPencilPreferredAction) -> Void

    func makeUIView(context: Context) -> SketchCanvasView {
        let view = SketchCanvasView()
        view.studio = studio
        view.onPencilTap = onPencilTap
        view.onPencilSqueeze = onPencilSqueeze
        return view
    }

    func updateUIView(_ view: SketchCanvasView, context: Context) {
        view.studio = studio
        view.onPencilTap = onPencilTap
        view.onPencilSqueeze = onPencilSqueeze
        view.presentSurface()
        view.setNeedsDisplay()
    }
}
