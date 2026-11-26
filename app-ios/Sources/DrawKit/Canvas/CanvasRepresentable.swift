// The SwiftUI host for the iPad canvas. The UIView stays alive across updates; a revision change
// only asks it to present the painting the studio already holds.

import SwiftUI

struct CanvasView: UIViewRepresentable {
    let studio: Studio
    let revision: Int
    let onPencilDoubleTap: () -> Void

    func makeUIView(context: Context) -> SketchCanvasView {
        let view = SketchCanvasView()
        view.studio = studio
        view.onPencilDoubleTap = onPencilDoubleTap
        return view
    }

    func updateUIView(_ view: SketchCanvasView, context: Context) {
        view.studio = studio
        view.onPencilDoubleTap = onPencilDoubleTap
        view.presentSurface()
        view.setNeedsDisplay()
    }
}
