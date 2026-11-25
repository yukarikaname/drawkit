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
    var onionImage: CGImage?
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
    var rotationRequest = 0
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
    var gpuStroke = false
    /// A smudge is being committed to the resident tiles segment by segment.
    var gpuSmudge = false
    /// The ink a tile commit writes: encoded for eight-bit, linear Display P3 for half floats.
    var gpuInk: (Float, Float, Float) = (0, 0, 0)
    var commitTimer: Timer?
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
    /// View rotation in radians, clockwise on screen. The document stays upright; only the
    /// presentation and the input mapping rotate.
    var rotation: CGFloat = 0
    var onRotation: ((CGFloat) -> Void)?
    /// Selection drag anchors in view points, mapped through the current rotation.
    var selectionCurrent: CGPoint?
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
        scheduleBenchmarkIfRequested()
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
        let corners = [CGPoint(x: 0, y: 0), CGPoint(x: bounds.width, y: 0),
                       CGPoint(x: bounds.width, y: bounds.height),
                       CGPoint(x: 0, y: bounds.height)]
        let points = corners.map(canvasPoint)
        let xs = points.map { $0.x / CGFloat(model.width) }
        let ys = points.map { $0.y / CGFloat(model.height) }
        let x0 = min(max(xs.min() ?? 0, 0), 1)
        let x1 = min(max(xs.max() ?? 1, 0), 1)
        let y0 = min(max(ys.min() ?? 0, 0), 1)
        let y1 = min(max(ys.max() ?? 1, 0), 1)
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

    /// Window point to an unclamped image pixel. The view is not flipped, so y counts up while
    /// rows count down, and the point is rotated back around the paper centre first.
    func canvasPoint(_ point: CGPoint) -> CGPoint {
        guard let model else { return .zero }
        let rect = imageRect
        let c = cos(rotation)
        let s = sin(rotation)
        let dx = point.x - rect.midX
        let dy = point.y - rect.midY
        let localX = dx * c + dy * s
        let localY = -dx * s + dy * c
        let scale = rect.width / CGFloat(model.width)
        guard scale > 0 else { return .zero }
        return CGPoint(x: CGFloat(model.width) / 2 + localX / scale,
                       y: CGFloat(model.height) / 2 - localY / scale)
    }

    /// Image pixel to window point, the inverse of `canvasPoint`.
    func viewPoint(_ pixel: CGPoint) -> CGPoint {
        guard let model else { return .zero }
        let rect = imageRect
        let scale = rect.width / CGFloat(model.width)
        let lx = (pixel.x - CGFloat(model.width) / 2) * scale
        let ly = (CGFloat(model.height) / 2 - pixel.y) * scale
        let c = cos(rotation)
        let s = sin(rotation)
        return CGPoint(x: rect.midX + lx * c - ly * s,
                       y: rect.midY + lx * s + ly * c)
    }

    /// Window point to image pixel. Returns nil outside the sheet, including after rotation.
    func imagePoint(_ point: CGPoint) -> CGPoint? {
        guard let model, imageRect.width > 0 else { return nil }
        let pixel = canvasPoint(point)
        guard pixel.x >= 0, pixel.y >= 0,
              pixel.x <= CGFloat(model.width - 1), pixel.y <= CGFloat(model.height - 1) else {
            return nil
        }
        return pixel
    }

    /// Draw in paper space; the canvas shader uses the same clockwise rotation.
    func applyRotation(_ context: CGContext) {
        guard rotation != 0 else { return }
        let centre = CGPoint(x: imageRect.midX, y: imageRect.midY)
        context.translateBy(x: centre.x, y: centre.y)
        context.rotate(by: rotation)
        context.translateBy(x: -centre.x, y: -centre.y)
    }
}
