// The canvas: a CAMetalLayer presented by wgpu, with a transparent NSView above it for the figure,
// the brush ring and the paper border. Input stays on this view, so gestures and tracking are the
// same as before the surface existed.

import SwiftUI
import AppKit
import QuartzCore
import QuartzCore

enum Tool: String, CaseIterable, Identifiable {
    case brush = "Brush"
    case eraser = "Eraser"
    case smudge = "Smudge"
    case fill = "Fill"
    case eyedropper = "Eyedropper"
    case select = "Select"
    case figure = "Figure"
    var id: String { rawValue }
}

final class CanvasView: NSView {
    var model: PaintModel?
    var figure: FigureModel?
    var tool: Tool = .brush
    var ink = Ink()
    var radius: CGFloat = 8
    var smudgeStrength: Double = 0.6
    var tolerance: Float = 0.12
    var fillClose = 1
    var fillExpand = 1
    var fillFeather = 0
    var mirrorEnabled = false
    var mirrorAngle: Double = 90
    var mirrorBoth = false
    var colorSpace: PaintColorSpace = .sRGB
    var pressureCurve: Double = 1.0
    var pressureLow: Double = 0
    var pressureHigh: Double = 1
    /// The linear extended path, not the document's HDR flag: Adobe RGB uses it too.
    var extended = false
    var surfaceExtended = false
    var exposure: Double = 0
    var onTool: ((Tool) -> Void)?
    var onSize: ((CGFloat) -> Void)?
    var onPick: ((Color, Double) -> Void)?
    var onZoom: ((CGFloat) -> Void)?
    var onPan: ((CGPoint) -> Void)?
    var onViewport: ((CGRect) -> Void)?
    private var reportedViewport: CGRect?
    var fitRequest = 0
    var navigateRequest = 0
    var selectionStart: CGPoint?
    var selectionDraft: CGRect?
    var onTabletDetected: (() -> Void)?
    var onBrush: ((BrushKind) -> Void)?
    var onColor: ((Color) -> Void)?
    var recentColours: [Color] = []
    var currentBrush: BrushKind = .softRound
    var tabletPrimary: TabletButtonAction = .quickMenu
    var tabletSecondary: TabletButtonAction = .pan
    var shortcuts: ShortcutStore?
    var quickMenuHost: NSView?
    var quickMenuState: QuickMenuState?
    var quickMenuKeyMonitor: Any?
    var panButton: Int?
    var panLastPoint: CGPoint?
    var tabletReported = false

    var last: CGPoint?
    var posing = false
    var cursorPoint: CGPoint?
    var eyedropping = false
    var stroking = false
    /// Timestamp of the last sample that actually moved, for hold-to-snap.
    var lastMoveTime: TimeInterval = 0
    var pressTimer: Timer?
    var downPoint: CGPoint?
    var downPixel: CGPoint?
    var downDynamics: (pressure: CGFloat, tilt: CGPoint, azimuth: CGFloat)?
    var pressMoved = false
    var pressPicked = false
    var tracking: NSTrackingArea?
    var zoom: CGFloat = 1
    var pan: CGPoint = .zero
    /// A two-finger scroll can be a pan or a flick. The flick is only decided when the fingers
    /// lift: short, horizontal and fast becomes undo/redo; anything slower stays a pan.
    var flick: (start: TimeInterval, dx: CGFloat, dy: CGFloat, pan: CGPoint)?
    var swallowMomentum = false
    var zoomAnimation: (from: CGFloat, to: CGFloat, start: TimeInterval, duration: TimeInterval)?
    var zoomTimer: Timer?
    let overlay = CanvasOverlayView()
    var surface: PaintSurface?
    var reportedSurface = false
    var gpuLayerCount = 0
    var gpuFailures = 0
    var gpuRetry: DispatchWorkItem?

    override var acceptsFirstResponder: Bool { true }

    override init(frame frameRect: NSRect) {
        super.init(frame: frameRect)
        // The window server merges tablet samples into one drag event by default. For a paint app
        // the merged samples are the line: dropping them is what makes a fast stroke a polyline.
        NSEvent.isMouseCoalescingEnabled = false
        wantsLayer = true
        overlay.canvas = self
        overlay.autoresizingMask = [.width, .height]
        addSubview(overlay)
        pressureConfiguration = NSPressureConfiguration(pressureBehavior: .primaryDeepClick)
    }

    required init?(coder: NSCoder) {
        super.init(coder: coder)
        NSEvent.isMouseCoalescingEnabled = false
        wantsLayer = true
        overlay.canvas = self
        overlay.autoresizingMask = [.width, .height]
        addSubview(overlay)
        pressureConfiguration = NSPressureConfiguration(pressureBehavior: .primaryDeepClick)
    }

    deinit {
        NotificationCenter.default.removeObserver(self)
    }

    override func makeBackingLayer() -> CALayer {
        let layer = CAMetalLayer()
        layer.isOpaque = false
        return layer
    }

    override func viewDidMoveToWindow() {
        super.viewDidMoveToWindow()
        NotificationCenter.default.removeObserver(self)
        if window == nil {
            stopZoomAnimation()
        }
        window?.makeFirstResponder(self)
        createSurface()
        if let window {
            NotificationCenter.default.addObserver(
                self, selector: #selector(windowStateChanged),
                name: NSWindow.didBecomeKeyNotification, object: window
            )
            NotificationCenter.default.addObserver(
                self, selector: #selector(windowStateChanged),
                name: NSWindow.didChangeOcclusionStateNotification, object: window
            )
            Task { @MainActor [weak self] in
                try? await Task.sleep(nanoseconds: 250_000_000)
                NSApp.activate()
                window.makeKeyAndOrderFront(nil)
                self?.presentSurface(force: true)
            }
        }
    }

    @objc private func windowStateChanged(_ notification: Notification) {
        presentSurface(force: true, upload: true)
    }

    override func layout() {
        super.layout()
        overlay.frame = bounds
        guard let metal = layer as? CAMetalLayer else { return }
        metal.contentsScale = window?.backingScaleFactor ?? 2
        let size = convertToBacking(bounds).size
        guard size.width > 0, size.height > 0 else { return }
        metal.drawableSize = size
        createSurface()
        surface?.resize(width: Int(size.width), height: Int(size.height))
        presentSurface(force: true, upload: false)
        reportViewport()
        overlay.needsDisplay = true
    }

    override func updateTrackingAreas() {
        super.updateTrackingAreas()
        if let tracking {
            removeTrackingArea(tracking)
        }
        let area = NSTrackingArea(
            rect: bounds,
            options: [.mouseMoved, .mouseEnteredAndExited, .activeInKeyWindow, .inVisibleRect],
            owner: self,
            userInfo: nil
        )
        addTrackingArea(area)
        tracking = area
    }

    override func mouseMoved(with event: NSEvent) {
        cursorPoint = convert(event.locationInWindow, from: nil)
        overlay.needsDisplay = true
    }

    override func mouseExited(with event: NSEvent) {
        cursorPoint = nil
        overlay.needsDisplay = true
    }

    /// Where the document sits in the view, after fitting it, zooming it, and moving it.
    var imageRect: CGRect {
        guard let model else { return .zero }
        let fit = min(bounds.width / CGFloat(model.width), bounds.height / CGFloat(model.height))
        let scale = fit * zoom
        let size = CGSize(width: CGFloat(model.width) * scale, height: CGFloat(model.height) * scale)
        return CGRect(x: (bounds.width - size.width) / 2 + pan.x,
                      y: (bounds.height - size.height) / 2 + pan.y,
                      width: size.width, height: size.height)
    }

    /// The figure is a reference on the paper, so it shares the canvas rectangle rather than the
    /// whole view.
    var figureRect: CGRect {
        guard let figure, figure.visible else { return .zero }
        let canvas = imageRect
        let height = canvas.height * 0.9
        let width = height * 2.0 / 3.0
        return CGRect(x: canvas.midX - width / 2, y: canvas.midY - height / 2,
                      width: width, height: height)
    }

    /// The part of the document currently under the view, normalized to 0...1. The navigator
    /// draws this as its viewport hint; it is clamped when the view shows more than the sheet.
    func visibleDocumentRect() -> CGRect {
        guard let model, bounds.width > 0, bounds.height > 0 else {
            return CGRect(x: 0, y: 0, width: 1, height: 1)
        }
        let rect = imageRect
        let scale = rect.width / CGFloat(model.width)
        guard scale > 0 else { return CGRect(x: 0, y: 0, width: 1, height: 1) }
        let left = -rect.minX / scale / CGFloat(model.width)
        let right = (bounds.width - rect.minX) / scale / CGFloat(model.width)
        let top = (CGFloat(model.height) - (bounds.height - rect.minY) / scale)
            / CGFloat(model.height)
        let bottom = (CGFloat(model.height) + rect.minY / scale) / CGFloat(model.height)
        let x0 = min(max(min(left, right), 0), 1)
        let x1 = min(max(max(left, right), 0), 1)
        let y0 = min(max(min(top, bottom), 0), 1)
        let y1 = min(max(max(top, bottom), 0), 1)
        return CGRect(x: x0, y: y0, width: max(x1 - x0, 0), height: max(y1 - y0, 0))
    }

    func reportViewport() {
        let rect = visibleDocumentRect()
        if let last = reportedViewport,
           abs(last.minX - rect.minX) < 0.0005,
           abs(last.minY - rect.minY) < 0.0005,
           abs(last.width - rect.width) < 0.0005,
           abs(last.height - rect.height) < 0.0005 {
            return
        }
        reportedViewport = rect
        onViewport?(rect)
    }

    /// Window point to image pixel. The view is not flipped, so y counts up while rows count down.
    func imagePoint(_ point: CGPoint) -> CGPoint? {
        let rect = imageRect
        guard rect.width > 0, rect.contains(point), let model else { return nil }
        let scale = rect.width / CGFloat(model.width)
        let x = (point.x - rect.minX) / scale
        let y = CGFloat(model.height) - (point.y - rect.minY) / scale
        return CGPoint(x: min(max(x, 0), CGFloat(model.width - 1)),
                       y: min(max(y, 0), CGFloat(model.height - 1)))
    }

    /// Called by the transparent overlay. When the surface is live the painting and its surround
    /// are already on screen; only the figure, the handles and the cursor are drawn here.
    func drawOverlay(_ dirtyRect: NSRect) {
        guard let context = NSGraphicsContext.current?.cgContext else { return }
        let rect = imageRect
        if surface == nil {
            NSColor(red: 0.106, green: 0.098, blue: 0.090, alpha: 1).setFill()
            bounds.fill()
            context.saveGState()
            context.setShadow(offset: CGSize(width: 0, height: -3), blur: 18,
                              color: NSColor.black.withAlphaComponent(0.65).cgColor)
            context.setFillColor(NSColor.white.cgColor)
            context.fill(rect)
            context.restoreGState()
            if let image = model?.image {
                context.interpolationQuality = .none
                context.draw(image, in: rect)
            }
        }
        overlayFigure(context)
        if surface == nil {
            context.setStrokeColor(NSColor(white: 0.72, alpha: 1).cgColor)
            context.stroke(rect.insetBy(dx: -0.5, dy: -0.5))
        }
        drawMirrorGuides(context)
        drawSelection(context)
        drawBrushCursor(context)
        drawLoupe(context)
    }

    func drawSelection(_ context: CGContext) {
        guard let model, imageRect.size.width > 0 else { return }
        let scale = imageRect.width / CGFloat(model.width)
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
        }
        if let draft = selectionDraft {
            context.setLineDash(phase: 0, lengths: [4, 4])
            context.setStrokeColor(NSColor.white.withAlphaComponent(0.9).cgColor)
            context.stroke(draft)
        }
        context.restoreGState()
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
              let point = cursorPoint, imageRect.contains(point) else { return }
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
