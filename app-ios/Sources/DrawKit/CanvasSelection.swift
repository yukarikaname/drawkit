// The rectangular selection gesture and its marching-ants overlay.

import UIKit

extension SketchCanvasView {
    func drawSelection(_ context: CGContext) {
        if let studio, let selection = studio.selection {
            let paper = paperRect
            let scale = paper.width / CGFloat(studio.width)
            let frame = CGRect(x: paper.minX + selection.minX * scale,
                               y: paper.minY + selection.minY * scale,
                               width: selection.width * scale,
                               height: selection.height * scale)
            context.saveGState()
            context.setLineWidth(1.5)
            context.setLineDash(phase: 0, lengths: [5, 4])
            context.setStrokeColor(UIColor.white.cgColor)
            context.stroke(frame)
            context.restoreGState()
        }
        if let draft = selectionDraft {
            context.saveGState()
            context.setLineWidth(1)
            context.setLineDash(phase: 0, lengths: [4, 4])
            context.setStrokeColor(UIColor.white.withAlphaComponent(0.8).cgColor)
            context.stroke(draft)
            context.restoreGState()
        }
    }

    func finishSelection(_ draft: CGRect, studio: Studio) {
        defer {
            selectionStart = nil
            selectionDraft = nil
            setNeedsDisplay()
        }
        let a = documentPoint(CGPoint(x: draft.minX, y: draft.minY))
        let b = documentPoint(CGPoint(x: draft.maxX, y: draft.maxY))
        let rect = CGRect(x: min(a.x, b.x), y: min(a.y, b.y),
                          width: abs(b.x - a.x), height: abs(b.y - a.y))
        if rect.width < 2 || rect.height < 2 {
            studio.setSelection(nil)
        } else {
            studio.setSelection(rect)
        }
    }
}
