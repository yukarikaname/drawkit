// The iPad canvas. A UIView because Pencil samples - force, altitude, azimuth, coalesced touches
// and hover - arrive here before SwiftUI sees anything. The painting is presented by the shared
// wgpu surface on a CAMetalLayer; the overlay above it draws only the cursor and mirror guides.
// If the surface cannot start, the same view falls back to drawing the core's CGImage.

import UIKit
import SwiftUI
import QuartzCore

final class SketchCanvasView: UIView {
    var studio: Studio?
    var onPencilDoubleTap: (() -> Void)?

    private let metalView = MetalCanvasView()
    private let overlay = CanvasOverlayView()
    var surface: PaintSurface?
    private var requestedExtended = false

    private var zoom: CGFloat = 1
    private var pan = CGPoint.zero
    var lastPoint: CGPoint?
    private var lastMoveTime: CFTimeInterval = 0
    var drawing = false
    var gpuStroke = false
    var commitTimer: Timer?

    private var cursor: CGPoint?
    var selectionStart: CGPoint?
    var selectionDraft: CGRect?
    private var pinch: UIPinchGestureRecognizer?
    private var twoFingerPan: UIPanGestureRecognizer?

    override init(frame: CGRect) {
        super.init(frame: frame)
        backgroundColor = UIColor(red: 0.106, green: 0.098, blue: 0.090, alpha: 1)
        isMultipleTouchEnabled = true
        contentMode = .redraw
        metalView.isUserInteractionEnabled = false
        overlay.isUserInteractionEnabled = false
        overlay.canvas = self
        addSubview(metalView)
        addSubview(overlay)
        installGestures()
        installPencil()
        installHover()
    }

    required init?(coder: NSCoder) {
        fatalError("init(coder:) is not used")
    }

    override func didMoveToWindow() {
        super.didMoveToWindow()
        if window != nil {
            updateSurface()
            presentSurface(force: true, upload: false)
        }
    }

    override func layoutSubviews() {
        super.layoutSubviews()
        metalView.frame = bounds
        overlay.frame = bounds
        updateSurface()
        presentSurface(force: true, upload: false)
    }

    // MARK: Display

    private var fitScale: CGFloat {
        let pad: CGFloat = 28
        return max(0.01, min((bounds.width - pad * 2) / CGFloat(studio?.width ?? 1024),
                             (bounds.height - pad * 2) / CGFloat(studio?.height ?? 1024)))
    }

    var paperRect: CGRect {
        let scale = fitScale * zoom
        let side = CGFloat(studio?.width ?? 1024) * scale
        return CGRect(x: bounds.midX + pan.x - side / 2,
                      y: bounds.midY + pan.y - side / 2,
                      width: side, height: side)
    }

    /// Fallback drawing only. When the surface is live it owns the paper, the painting and the
    /// surround, and this layer stays empty underneath it.
    override func draw(_ rect: CGRect) {
        guard surface == nil, let context = UIGraphicsGetCurrentContext(), let studio else {
            return
        }
        backgroundColor?.setFill()
        context.fill(bounds)
        let paper = paperRect
        context.saveGState()
        context.setShadow(offset: CGSize(width: 0, height: 6), blur: 26,
                          color: UIColor.black.withAlphaComponent(0.55).cgColor)
        UIColor.white.setFill()
        context.fill(paper)
        context.restoreGState()
        if let image = studio.image {
            UIImage(cgImage: image).draw(in: paper)
        }
        UIColor.white.withAlphaComponent(0.14).setStroke()
        context.stroke(paper.insetBy(dx: -0.5, dy: -0.5))
        drawChrome(context)
    }

    func drawChrome(_ context: CGContext) {
        drawSelection(context)
        guard let studio else { return }
        let paper = paperRect
        if let onion = studio.onionImage {
            UIImage(cgImage: onion).draw(in: paper)
        }
        if studio.mirror != 0 {
            context.saveGState()
            context.setStrokeColor(UIColor.systemOrange.withAlphaComponent(0.75).cgColor)
            context.setLineWidth(1)
            context.setLineDash(phase: 0, lengths: [5, 5])
            if studio.mirror == 1 || studio.mirror == 3 {
                context.move(to: CGPoint(x: paper.midX, y: paper.minY))
                context.addLine(to: CGPoint(x: paper.midX, y: paper.maxY))
            }
            if studio.mirror == 2 || studio.mirror == 3 {
                context.move(to: CGPoint(x: paper.minX, y: paper.midY))
                context.addLine(to: CGPoint(x: paper.maxX, y: paper.midY))
            }
            context.strokePath()
            context.restoreGState()
        }
        if let cursor, studio.tool != .fill {
            let radius = CGFloat(studio.brushSize) / 2 * fitScale * zoom
            if radius >= 1.5 {
                let ring = CGRect(x: cursor.x - radius, y: cursor.y - radius,
                                  width: radius * 2, height: radius * 2)
                UIColor.black.withAlphaComponent(0.6).setStroke()
                context.setLineWidth(1)
                context.strokeEllipse(in: ring.insetBy(dx: -0.5, dy: -0.5))
                UIColor.white.withAlphaComponent(0.9).setStroke()
                context.strokeEllipse(in: ring)
            }
        }
    }

    // MARK: Surface

    private func updateSurface() {
        guard bounds.width > 0, bounds.height > 0 else { return }
        let requested = studio?.needsLinear ?? false
        if requested != requestedExtended {
            requestedExtended = requested
            surface = nil
        }
        let scale = max(contentScaleFactor, 1)
        let width = Int(bounds.width * scale)
        let height = Int(bounds.height * scale)
        if surface == nil, let metal = metalView.layer as? CAMetalLayer {
            metal.contentsScale = scale
            metal.drawableSize = CGSize(width: width, height: height)
            surface = PaintSurface(layer: metal, width: width, height: height,
                                   extended: requested,
                                   colorSpace: studio?.colorSpace ?? .sRGB)
            studio?.paintSurface = surface
            if surface != nil {
                studio?.markSurfaceFull()
            }
        }
        surface?.resize(width: width, height: height)
        surface?.setExposure(Float(studio?.exposure ?? 0))
        metalView.isHidden = surface == nil
    }

    /// Upload the dirty rectangle if there is one. Pan and zoom force the call because the paper
    /// rectangle is a uniform even when the pixels did not move.
    func presentSurface(force: Bool = false, upload: Bool = true) {
        if upload { studio?.materializeCanvas() }
        updateSurface()
        guard let studio, let surface else {
            overlay.setNeedsDisplay()
            return
        }
        let dirty = upload ? studio.takeSurfaceDirty() : nil
        if dirty == nil && !force {
            overlay.setNeedsDisplay()
            return
        }
        let scale = max(contentScaleFactor, 1)
        let rect = paperRect
        let paper = CGRect(x: rect.minX * scale,
                           y: (bounds.height - rect.maxY) * scale,
                           width: rect.width * scale,
                           height: rect.height * scale)
        let dirtyRect = dirty.map {
            CGRect(x: $0.x, y: $0.y, width: $0.width, height: $0.height)
        }
        studio.withSurfacePixels(hdr: surface.extended) { pixels, width, height, linear in
            let presented = surface.present(pixels: pixels, width: width, height: height,
                                            paper: paper, dirty: dirtyRect, linear: linear)
            if !presented {
                studio.markSurfaceFull()
            }
        }
        overlay.setNeedsDisplay()
    }

    func redraw() {
        if surface == nil {
            setNeedsDisplay()
        }
        presentSurface()
    }

    func recentre() {
        zoom = 1
        pan = .zero
        presentSurface(force: true, upload: false)
    }

    // MARK: Touches

    override func touchesBegan(_ touches: Set<UITouch>, with event: UIEvent?) {
        guard let studio else { return }
        if (event?.allTouches?.count ?? 1) > 1 {
            cancelStroke()
            return
        }
        guard let touch = touches.first else { return }
        cursor = touch.location(in: self)
        if studio.tool == .select {
            selectionStart = cursor
            selectionDraft = CGRect(origin: cursor!, size: .zero)
            setNeedsDisplay()
            return
        }
        let point = documentPoint(cursor!)
        guard inside(point) else { return }
        if studio.tool == .eyedropper {
            _ = studio.pick(at: point)
            redraw()
            return
        }
        if studio.tool == .fill {
            studio.beginFill(at: point)
            redraw()
            return
        }
        if gpuStroke {
            // A stroke still waiting for its slices must land before this one changes the layer.
            studio.flushDeferred()
            finishGPUStroke()
        }
        if studio.tool == .brush, let surface, gpuStrokeEligible {
            studio.setDeferredRaster(true)
            let ink = studio.colorComponents()
            surface.strokeBegin(width: studio.width, height: studio.height,
                                color: (ink.0, ink.1, ink.2, 1),
                                flow: dk_brush_param(studio.handle, 1), pressureAlpha: dk_brush_param(studio.handle, 3),
                                paper: dk_brush_param(studio.handle, 9), tip: dk_brush_param(studio.handle, 10))
            gpuStroke = true
        }
        studio.beginStroke()
        drawing = true
        lastPoint = point
        lastMoveTime = CACurrentMediaTime()
        redraw()
    }

    override func touchesMoved(_ touches: Set<UITouch>, with event: UIEvent?) {
        if let start = selectionStart, let touch = touches.first {
            let point = touch.location(in: self)
            selectionDraft = CGRect(x: min(start.x, point.x), y: min(start.y, point.y),
                                    width: abs(point.x - start.x), height: abs(point.y - start.y))
            cursor = point
            setNeedsDisplay()
            return
        }
        guard drawing, let studio, let touch = touches.first else { return }
        cursor = touch.location(in: self)
        let samples = event?.coalescedTouches(for: touch) ?? [touch]
        for sample in samples {
            let point = documentPoint(sample.location(in: self))
            guard inside(point) else { continue }
            guard let from = lastPoint else {
                lastPoint = point
                continue
            }
            let dynamics = pencilDynamics(of: sample)
            studio.segment(from: from, to: point, pressure: dynamics.pressure,
                           tilt: dynamics.tilt, azimuth: dynamics.azimuth, roll: dynamics.roll)
            if gpuStroke, let surface {
                let p = Float(dynamics.pressure)
                let pressureRadius = dk_brush_param(studio.handle, 2)
                let painted = Float(studio.brushSize / 2)
                    * (pressureRadius + (1 - pressureRadius) * p)
                let lean = hypot(dynamics.tilt[0], dynamics.tilt[1])
                let strength = min(max(lean <= 1 ? lean : lean / 60, 0), 1)
                let direction = lean > 0.01
                    ? atan2(dynamics.tilt[1], dynamics.tilt[0]) * 180 / .pi
                    : dynamics.azimuth * 180 / .pi
                surface.strokeSegment(
                    fromX: Float(from.x), fromY: Float(from.y),
                    toX: Float(point.x), toY: Float(point.y),
                    radius: painted,
                    hardness: dk_brush_param(studio.handle, 0),
                    spacing: dk_brush_param(studio.handle, 4),
                    roundness: dk_brush_param(studio.handle, 7) * (1 - 0.85 * strength),
                    angle: dk_brush_param(studio.handle, 6) + direction,
                    pressure: p)
            }
            lastPoint = point
            lastMoveTime = CACurrentMediaTime()
        }
        redraw()
    }

    override func touchesEnded(_ touches: Set<UITouch>, with event: UIEvent?) {
        if let draft = selectionDraft, let studio {
            finishSelection(draft, studio: studio)
            return
        }
        guard drawing, let studio else { return }
        if lastPoint != nil {
            let held = CACurrentMediaTime() - lastMoveTime >= 0.35
            studio.endStroke(quickShape: held)
            if !startTileCommit() {
                studio.materializeCanvas()
                startGPUCommit()
            }
        } else {
            studio.cancelStroke()
        }
        drawing = false
        lastPoint = nil
        if let touch = touches.first {
            cursor = touch.location(in: self)
        }
        redraw()
    }

    override func touchesCancelled(_ touches: Set<UITouch>, with event: UIEvent?) {
        selectionStart = nil
        selectionDraft = nil
        cancelStroke()
    }

    // MARK: Gestures

    private func installGestures() {
        let undo = UITapGestureRecognizer(target: self, action: #selector(twoFingerUndo))
        undo.numberOfTouchesRequired = 2
        undo.cancelsTouchesInView = false
        addGestureRecognizer(undo)

        let redo = UITapGestureRecognizer(target: self, action: #selector(threeFingerRedo))
        redo.numberOfTouchesRequired = 3
        redo.cancelsTouchesInView = false
        addGestureRecognizer(redo)

        let pinch = UIPinchGestureRecognizer(target: self, action: #selector(handlePinch(_:)))
        pinch.cancelsTouchesInView = false
        pinch.delegate = self
        addGestureRecognizer(pinch)
        self.pinch = pinch

        let pan = UIPanGestureRecognizer(target: self, action: #selector(handlePan(_:)))
        pan.minimumNumberOfTouches = 2
        pan.maximumNumberOfTouches = 2
        pan.cancelsTouchesInView = false
        pan.delegate = self
        addGestureRecognizer(pan)
        twoFingerPan = pan
    }

    @objc private func twoFingerUndo() {
        studio?.undo()
        redraw()
    }

    @objc private func threeFingerRedo() {
        studio?.redo()
        redraw()
    }

    @objc private func handlePinch(_ gesture: UIPinchGestureRecognizer) {
        if gesture.state == .began { cancelStroke() }
        guard gesture.state == .changed else { return }
        zoom = min(max(zoom * gesture.scale, 0.35), 10)
        gesture.scale = 1
        presentSurface(force: true, upload: false)
    }

    @objc private func handlePan(_ gesture: UIPanGestureRecognizer) {
        if gesture.state == .began { cancelStroke() }
        guard gesture.state == .changed else { return }
        let translation = gesture.translation(in: self)
        pan.x += translation.x
        pan.y += translation.y
        gesture.setTranslation(.zero, in: self)
        presentSurface(force: true, upload: false)
    }

    // MARK: Pencil and hover

    private func installPencil() {
        let interaction = UIPencilInteraction()
        interaction.delegate = self
        addInteraction(interaction)
    }

    private func installHover() {
        let hover = UIHoverGestureRecognizer(target: self, action: #selector(handleHover(_:)))
        hover.cancelsTouchesInView = false
        addGestureRecognizer(hover)
    }

    @objc private func handleHover(_ gesture: UIHoverGestureRecognizer) {
        if gesture.state == .ended {
            cursor = nil
        } else {
            cursor = gesture.location(in: self)
        }
        if surface == nil {
            setNeedsDisplay()
        }
        overlay.setNeedsDisplay()
    }
}

/// The CAMetalLayer host. wgpu configures the layer; the view only keeps it the size of the canvas.
private final class MetalCanvasView: UIView {
    override class var layerClass: AnyClass { CAMetalLayer.self }

    override init(frame: CGRect) {
        super.init(frame: frame)
        isOpaque = true
    }

    required init?(coder: NSCoder) {
        fatalError("init(coder:) is not used")
    }
}

/// Cursor and mirror guides above the presented painting. Transparent, and never takes touches.
private final class CanvasOverlayView: UIView {
    weak var canvas: SketchCanvasView?

    override init(frame: CGRect) {
        super.init(frame: frame)
        isOpaque = false
        backgroundColor = .clear
    }

    required init?(coder: NSCoder) {
        fatalError("init(coder:) is not used")
    }

    override func draw(_ rect: CGRect) {
        guard let context = UIGraphicsGetCurrentContext() else { return }
        canvas?.drawChrome(context)
    }
}

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
