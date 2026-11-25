// Overlay drawing above the Metal layer: selection, onion, mirror guides, the figure, the
// cursor and the loupe. These are transient chrome, so AppKit drawing is cheaper than shaders.

import SwiftUI
import AppKit

extension CanvasView {
    /// Called by the transparent overlay. When the surface is live the painting and its surround
    /// are already on screen; only the figure, the handles and the cursor are drawn here.
    func drawOverlay(_ dirtyRect: NSRect) {
        guard let context = NSGraphicsContext.current?.cgContext else { return }
        let rect = imageRect
        // The painting is always the Metal layer below. Without a surface there is no stale CPU
        // image to show instead, so only the surround and the chrome are drawn.
        if surface == nil {
            NSColor(red: 0.106, green: 0.098, blue: 0.090, alpha: 1).setFill()
            bounds.fill()
        }
        context.saveGState()
        applyRotation(context)
        if let onionImage {
            context.saveGState()
            context.draw(onionImage, in: rect)
            context.restoreGState()
        }
        drawMirrorGuides(context)
        drawSelection(context)
        drawSelectionDraft(context)
        context.restoreGState()
        // The figure stays upright while the paper turns: its handles are hit-tested in view
        // space, and a reference figure is easier to pose when it does not spin.
        overlayFigure(context)
        drawBrushCursor(context)
        drawLoupe(context)
    }

    func drawSelection(_ context: CGContext) {
        guard let model, imageRect.size.width > 0 else { return }
        let scale = imageRect.width / CGFloat(model.width)
        var hint: (frame: CGRect, width: Int, height: Int)?
        context.saveGState()
        context.setLineWidth(1)
        context.setLineDash(phase: 0, lengths: [4, 4])
        context.setStrokeColor(NSColor.black.withAlphaComponent(0.8).cgColor)
        if let selection = model.selection {
            let frame = CGRect(x: imageRect.minX + CGFloat(selection.x) * scale,
                               y: imageRect.minY + CGFloat(model.height - selection.y - selection.height) * scale,
                               width: CGFloat(selection.width) * scale,
                               height: CGFloat(selection.height) * scale)
            context.stroke(frame)
            context.setLineDash(phase: 4, lengths: [4, 4])
            context.setStrokeColor(NSColor.white.cgColor)
            context.stroke(frame)
            hint = (frame, selection.width, selection.height)
        }
        if let selected = selectedStroke {
            let scale = imageRect.width / CGFloat(model.width)
            let frame = CGRect(
                x: imageRect.minX + CGFloat(selected.bounds.x) * scale,
                y: imageRect.minY
                    + CGFloat(model.height - selected.bounds.y - selected.bounds.height) * scale,
                width: CGFloat(selected.bounds.width) * scale,
                height: CGFloat(selected.bounds.height) * scale)
            context.saveGState()
            context.setLineWidth(1)
            context.setLineDash(phase: 0, lengths: [5, 4])
            context.setStrokeColor(NSColor.systemOrange.withAlphaComponent(0.9).cgColor)
            context.stroke(frame)
            context.restoreGState()
        }
        context.restoreGState()
        if let hint, hint.width > 0, hint.height > 0 {
            drawSelectionHint("\(hint.width) × \(hint.height)", near: hint.frame)
        }
    }

    /// The in-progress drag is defined by two view points, so it is mapped back into paper space
    /// and drawn with the same rotation as the painting.
    func drawSelectionDraft(_ context: CGContext) {
        guard let model, let start = selectionStart, let current = selectionCurrent else { return }
        let a = canvasPoint(start)
        let b = canvasPoint(current)
        let x0 = min(max(min(a.x, b.x), 0), CGFloat(model.width))
        let x1 = min(max(max(a.x, b.x), 0), CGFloat(model.width))
        let y0 = min(max(min(a.y, b.y), 0), CGFloat(model.height))
        let y1 = min(max(max(a.y, b.y), 0), CGFloat(model.height))
        let scale = imageRect.width / CGFloat(model.width)
        let rect = imageRect
        let frame = CGRect(x: rect.minX + x0 * scale,
                           y: rect.minY + CGFloat(model.height) * scale - y1 * scale,
                           width: max(x1 - x0, 0) * scale,
                           height: max(y1 - y0, 0) * scale)
        context.saveGState()
        context.setLineWidth(1)
        context.setLineDash(phase: 0, lengths: [4, 4])
        context.setStrokeColor(NSColor.white.withAlphaComponent(0.9).cgColor)
        context.stroke(frame)
        context.restoreGState()
    }

    /// A live size readout while the selection is being dragged, not just after release.
    private func drawSelectionHint(_ text: String, near frame: CGRect) {
        let attributes: [NSAttributedString.Key: Any] = [
            .font: NSFont.monospacedDigitSystemFont(ofSize: 10, weight: .medium),
            .foregroundColor: NSColor.white,
        ]
        let size = text.size(withAttributes: attributes)
        let margin: CGFloat = 4
        var origin = CGPoint(x: frame.minX, y: frame.minY - size.height - 6)
        origin.x = min(max(origin.x, margin), bounds.width - size.width - margin * 2)
        origin.y = min(max(origin.y, margin), bounds.height - size.height - margin * 2)
        let plate = CGRect(x: origin.x - margin, y: origin.y - 2,
                           width: size.width + margin * 2, height: size.height + 4)
        NSColor.black.withAlphaComponent(0.68).setFill()
        NSBezierPath(roundedRect: plate, xRadius: 4, yRadius: 4).fill()
        text.draw(at: origin, withAttributes: attributes)
    }

    func drawMirrorGuides(_ context: CGContext) {
        guard mirrorEnabled else { return }
        let paper = imageRect
        context.saveGState()
        context.clip(to: paper)
        context.setStrokeColor(NSColor(calibratedRed: 0.886, green: 0.337,
                                       blue: 0.243, alpha: 0.28).cgColor)
        context.setLineWidth(1)
        context.setLineDash(phase: 0, lengths: [5, 5])
        let centre = CGPoint(x: paper.midX, y: paper.midY)
        let reach = max(paper.width, paper.height)
        var angles = [mirrorAngle]
        if mirrorBoth { angles.append(mirrorAngle + 90) }
        for degrees in angles {
            let radians = degrees * .pi / 180
            let dx = cos(radians) * reach
            let dy = sin(radians) * reach
            context.move(to: CGPoint(x: centre.x - dx, y: centre.y - dy))
            context.addLine(to: CGPoint(x: centre.x + dx, y: centre.y + dy))
        }
        context.strokePath()
        context.restoreGState()
    }

    func overlayFigure(_ context: CGContext) {
        guard let figure, figure.visible else { return }
        let frame = figureRect
        guard frame.width > 0 else { return }
        if let image = figure.image {
            context.saveGState()
            context.setAlpha(figure.opacity)
            context.interpolationQuality = .high
            context.draw(image, in: frame)
            context.restoreGState()
        }
        context.setFillColor(NSColor.systemOrange.withAlphaComponent(0.85).cgColor)
        for joint in FigureModel.draggableJoints {
            guard let point = figure.jointScreenPosition(joint, in: frame) else { continue }
            context.fillEllipse(in: CGRect(x: point.x - 4, y: point.y - 4, width: 8, height: 8))
        }
    }

    /// The eyedropper loupe: a magnified patch and its hex value, so a colour can be judged rather
    /// than guessed. It only exists while Option is held.
    func drawLoupe(_ context: CGContext) {
        guard eyedropping, let point = cursorPoint, let model,
              let centre = imagePoint(point) else { return }
        let samples = 7
        let cell: CGFloat = 12
        let side = CGFloat(samples) * cell
        var origin = CGPoint(x: point.x + 18, y: point.y - 18 - side)
        if origin.x + side > bounds.maxX { origin.x = point.x - 18 - side }
        if origin.y < bounds.minY { origin.y = point.y + 18 }
        let frame = CGRect(x: origin.x, y: origin.y, width: side, height: side)
        context.setFillColor(NSColor(white: 0.08, alpha: 0.94).cgColor)
        context.fill(frame)
        let baseX = Int(centre.x) - samples / 2
        let baseY = Int(centre.y) - samples / 2
        for row in 0..<samples {
            for column in 0..<samples {
                let x = min(max(baseX + column, 0), model.width - 1)
                let y = min(max(baseY + row, 0), model.height - 1)
                guard let color = model.pick(at: CGPoint(x: x, y: y)),
                      let ns = NSColor(color).usingColorSpace(.deviceRGB) else { continue }
                context.setFillColor(ns.cgColor)
                context.fill(CGRect(x: origin.x + CGFloat(column) * cell,
                                    y: origin.y + side - CGFloat(row + 1) * cell,
                                    width: cell, height: cell))
            }
        }
        context.setStrokeColor(NSColor.white.withAlphaComponent(0.8).cgColor)
        context.setLineWidth(1)
        context.stroke(frame)
        guard let color = model.pick(at: CGPoint(x: Int(centre.x), y: Int(centre.y))),
              let ns = NSColor(color).usingColorSpace(.deviceRGB) else { return }
        let hex = String(format: "#%02X%02X%02X", Int(ns.redComponent * 255),
                         Int(ns.greenComponent * 255), Int(ns.blueComponent * 255))
        let attributes: [NSAttributedString.Key: Any] = [
            .font: NSFont.monospacedSystemFont(ofSize: 9, weight: .regular),
            .foregroundColor: NSColor.white,
        ]
        NSAttributedString(string: hex, attributes: attributes)
            .draw(at: CGPoint(x: origin.x + 4, y: origin.y + side + 4))
    }

    /// A ring the size of the brush, so the painter sees what will be painted before pressing.
    func drawBrushCursor(_ context: CGContext) {
        guard tool == .brush || tool == .eraser || tool == .smudge,
              let point = cursorPoint, imagePoint(point) != nil else { return }
        let scale = imageRect.width / CGFloat(model?.width ?? 1)
        let ring = radius * scale
        guard ring >= 1.5 else { return }
        let rect = CGRect(x: point.x - ring, y: point.y - ring, width: ring * 2, height: ring * 2)
        context.setLineWidth(1)
        context.setStrokeColor(NSColor.black.withAlphaComponent(0.7).cgColor)
        context.strokeEllipse(in: rect.insetBy(dx: -0.5, dy: -0.5))
        context.setStrokeColor(NSColor.white.withAlphaComponent(0.9).cgColor)
        context.strokeEllipse(in: rect)
    }
}
