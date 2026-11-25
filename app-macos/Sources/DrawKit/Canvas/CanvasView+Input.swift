// Mouse, pen, pressure, the quick menu and tool keys.

import SwiftUI
import AppKit

extension CanvasView {
    // MARK: Mouse and pen

    override func mouseDown(with event: NSEvent) {
        window?.makeFirstResponder(self)
        let point = convert(event.locationInWindow, from: nil)
        cursorPoint = point
        lastMoveTime = event.timestamp
        if tool == .select {
            selectionStart = point
            selectionDraft = CGRect(origin: point, size: .zero)
            overlay.needsDisplay = true
            return
        }
        if tool == .figure {
            posing = figure?.beginPose(at: point, in: figureRect) ?? false
            last = point
            return
        }
        guard let model, let pixel = imagePoint(point) else {
            return
        }
        if tool == .eyedropper || event.modifierFlags.contains(.option) {
            eyedropping = true
            cursorPoint = point
            if let (color, glow) = model.pickInk(at: pixel) {
                onPick?(color, glow)
            }
            overlay.needsDisplay = true
            return
        }
        if tool == .fill && event.pointingDeviceType != .eraser {
            model.beginFill(at: pixel, tolerance: tolerance,
                            close: fillClose, expand: fillExpand, feather: fillFeather, ink: ink)
            presentSurface()
            return
        }
        // A click paints on release; holding still becomes an eyedropper. Deferring the stroke is
        // what makes a long press possible without painting a dot first.
        downPoint = point
        downPixel = pixel
        downDynamics = tabletDynamics(of: event)
        pressMoved = false
        pressPicked = false
        stroking = false
        pressTimer?.invalidate()
        pressTimer = Timer.scheduledTimer(timeInterval: 0.35, target: self,
                                          selector: #selector(longPressFired),
                                          userInfo: nil, repeats: false)
        overlay.needsDisplay = true
    }

    @objc private func longPressFired() {
        pressTimer = nil
        guard !pressMoved, !pressPicked, let model, let pixel = downPixel,
              let point = downPoint else { return }
        pressPicked = true
        eyedropping = true
        cursorPoint = point
        if let (color, glow) = model.pickInk(at: pixel) {
            onPick?(color, glow)
        }
        overlay.needsDisplay = true
    }

    override func mouseDragged(with event: NSEvent) {
        if tool == .select, let start = selectionStart {
            let point = convert(event.locationInWindow, from: nil)
            selectionDraft = CGRect(x: min(start.x, point.x), y: min(start.y, point.y),
                                    width: abs(point.x - start.x), height: abs(point.y - start.y))
            overlay.needsDisplay = true
            return
        }
        if eyedropping {
            let point = convert(event.locationInWindow, from: nil)
            cursorPoint = point
            if let pixel = imagePoint(point), let (color, glow) = model?.pickInk(at: pixel) {
                onPick?(color, glow)
            }
            overlay.needsDisplay = true
            return
        }
        if tool == .figure {
            let point = convert(event.locationInWindow, from: nil)
            if let previous = last {
                if posing {
                    figure?.dragPose(to: point, in: figureRect)
                } else {
                    figure?.orbit(dx: point.x - previous.x, dy: point.y - previous.y)
                }
            }
            last = point
            overlay.needsDisplay = true
            return
        }
        guard tool == .brush || tool == .eraser || tool == .smudge
                || event.pointingDeviceType == .eraser else {
            return
        }
        if !stroking {
            guard let downPoint, let downPixel else { return }
            let point = convert(event.locationInWindow, from: nil)
            guard hypot(point.x - downPoint.x, point.y - downPoint.y) > 2 else { return }
            pressMoved = true
            pressTimer?.invalidate()
            pressTimer = nil
            last = downPixel
            model?.beginStroke()
            stroking = true
        }
        draw(event)
    }

    func draw(_ event: NSEvent) {
        guard let previous = last,
              let pixel = imagePoint(convert(event.locationInWindow, from: nil)) else { return }
        if pixel != previous {
            lastMoveTime = event.timestamp
        }
        paint(from: previous, to: pixel, event: event)
        last = pixel
        presentSurface()
        cursorPoint = convert(event.locationInWindow, from: nil)
    }

    func paint(from: CGPoint, to: CGPoint, event: NSEvent) {
        guard let model else { return }
        let dynamics = tabletDynamics(of: event)
        let eraser = tool == .eraser || event.pointingDeviceType == .eraser
        if eraser {
            model.erase(from: from, to: to, radius: radius, pressure: dynamics.pressure,
                        tilt: dynamics.tilt, azimuth: dynamics.azimuth)
        } else if tool == .smudge {
            model.smudge(from: from, to: to, radius: radius,
                         pressure: dynamics.pressure, strength: smudgeStrength)
        } else {
            model.stroke(from: from, to: to, radius: radius, pressure: dynamics.pressure,
                         tilt: dynamics.tilt, azimuth: dynamics.azimuth, ink: ink)
        }
    }

    /// A tablet reports pen rotation and a tilt vector; a mouse has neither. The core stores them
    /// on the stroke so a future brush engine can use them, while today's round brush keeps using
    /// pressure only.
    func tabletDynamics(of event: NSEvent)
        -> (pressure: CGFloat, tilt: CGPoint, azimuth: CGFloat) {
        let pressure = pressure(of: event)
        guard event.subtype == .tabletPoint else {
            return (pressure, .zero, 0)
        }
        if !tabletReported {
            tabletReported = true
            onTabletDetected?()
        }
        let azimuth = CGFloat(event.rotation) * .pi / 180
        return (pressure, event.tilt, azimuth)
    }

    override func mouseUp(with event: NSEvent) {
        if tool == .select {
            finishSelection()
            return
        }
        pressTimer?.invalidate()
        pressTimer = nil
        if stroking {
            let held = event.timestamp - lastMoveTime >= 0.35
            model?.endStroke(quickShape: held)
        } else if !pressPicked, !eyedropping, let model, let pixel = downPixel {
            // A click without movement is still a dot, with the pressure from the press.
            let dynamics = downDynamics ?? (1, .zero, 0)
            if tool == .eraser || event.pointingDeviceType == .eraser {
                model.erase(from: pixel, to: pixel, radius: radius,
                            pressure: dynamics.pressure,
                            tilt: dynamics.tilt, azimuth: dynamics.azimuth)
            } else if tool == .smudge {
                // A smudge with no movement has nothing to pull from.
            } else {
                model.beginStroke()
                model.stroke(from: pixel, to: pixel, radius: radius,
                             pressure: dynamics.pressure,
                             tilt: dynamics.tilt, azimuth: dynamics.azimuth, ink: ink)
                model.endStroke()
            }
            presentSurface()
        }
        last = nil
        downPoint = nil
        downPixel = nil
        downDynamics = nil
        panButton = nil
        panLastPoint = nil
        stroking = false
        pressMoved = false
        posing = false
        eyedropping = false
        pressPicked = false
        overlay.needsDisplay = true
    }

    private func finishSelection() {
        defer {
            selectionStart = nil
            selectionDraft = nil
            overlay.needsDisplay = true
        }
        guard let model, let draft = selectionDraft else { return }
        let rect = imageRect
        guard rect.width > 0, rect.height > 0 else { return }
        let scale = rect.width / CGFloat(model.width)
        let x0 = (draft.minX - rect.minX) / scale
        let x1 = (draft.maxX - rect.minX) / scale
        let y0 = CGFloat(model.height) - (draft.maxY - rect.minY) / scale
        let y1 = CGFloat(model.height) - (draft.minY - rect.minY) / scale
        let left = min(max(x0, 0), CGFloat(model.width))
        let right = min(max(x1, 0), CGFloat(model.width))
        let top = min(max(y0, 0), CGFloat(model.height))
        let bottom = min(max(y1, 0), CGFloat(model.height))
        let width = Int((right - left).rounded())
        let height = Int((bottom - top).rounded())
        if width < 2 || height < 2 {
            model.clearSelection()
        } else {
            model.setSelection(SurfaceRect(x: Int(left.rounded()), y: Int(top.rounded()),
                                           width: width, height: height))
        }
    }

    /// Tablets report 0 to 1. A mouse reports 1 while its button is down and 0 otherwise, so the
    /// fallback keeps a mouse stroke at full size instead of shrinking it to the pressure floor.
    func pressure(of event: NSEvent) -> CGFloat {
        let value = CGFloat(event.pressure)
        let raw = Double(value > 0.01 ? value : 1.0)
        let span = max(pressureHigh - pressureLow, 0.01)
        let mapped = min(max((raw - pressureLow) / span, 0.0), 1.0)
        return CGFloat(pow(mapped, pressureCurve))
    }

    // MARK: Keys

    override func keyDown(with event: NSEvent) {
        guard let shortcuts,
              let action = shortcuts.action(for: event, scope: .canvas) else {
            super.keyDown(with: event)
            return
        }
        switch action {
        case .brush:
            onTool?(.brush)
        case .eraser:
            onTool?(.eraser)
        case .fill:
            onTool?(.fill)
        case .eyedropper:
            onTool?(.eyedropper)
        case .fit:
            fit()
        case .smaller:
            onSize?(max(1, radius - 2))
        case .larger:
            onSize?(min(48, radius + 2))
        default:
            super.keyDown(with: event)
        }
    }
}
