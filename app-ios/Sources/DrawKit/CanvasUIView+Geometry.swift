// Canvas-to-document mapping kept out of the main view file.

import UIKit

extension SketchCanvasView {
    func inside(_ point: CGPoint) -> Bool {
        guard let studio else { return false }
        return point.x >= 0 && point.y >= 0
            && point.x < CGFloat(studio.width) && point.y < CGFloat(studio.height)
    }

    func documentPoint(_ viewPoint: CGPoint) -> CGPoint {
        let paper = paperRect
        guard paper.width > 0 else { return .zero }
        let scale = CGFloat(studio?.width ?? 1024) / paper.width
        return CGPoint(x: (viewPoint.x - paper.minX) * scale,
                       y: (viewPoint.y - paper.minY) * scale)
    }
}
