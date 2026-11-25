// Zoom, pan, swipe and the tablet buttons that drive them.

import SwiftUI
import AppKit

extension CanvasView {
    // MARK: Zoom and pan

    override func magnify(with event: NSEvent) {
        if tool == .figure {
            figure?.zoom(by: Float(1 + event.magnification))
            overlay.needsDisplay = true
            return
        }
        zoom(by: 1 + event.magnification, around: convert(event.locationInWindow, from: nil))
    }

    override func scrollWheel(with event: NSEvent) {
        if tool == .figure {
            figure?.zoom(by: Float(1 + event.scrollingDeltaY * 0.01))
            overlay.needsDisplay = true
            return
        }
        if event.modifierFlags.contains(.command) {
            zoom(by: 1 + event.scrollingDeltaY * 0.01,
                 around: convert(event.locationInWindow, from: nil))
            return
        }
        // A mouse wheel has no phases and no gesture to read, so it stays a plain pan.
        guard event.hasPreciseScrollingDeltas else {
            pan.x -= event.scrollingDeltaX
            pan.y -= event.scrollingDeltaY
            panChanged()
            presentSurface(force: true, upload: false)
            return
        }
        if event.phase.contains(.began) {
            swallowMomentum = false
            flick = (event.timestamp, 0, 0, pan)
        }
        if swallowMomentum {
            if event.momentumPhase.contains(.ended) || event.momentumPhase.isEmpty {
                swallowMomentum = false
            }
            return
        }
        if event.phase.contains(.cancelled) {
            flick = nil
            return
        }
        // Momentum events continue after the fingers lift; the decision was already made.
        guard var gesture = flick, event.momentumPhase.isEmpty else {
            pan.x -= event.scrollingDeltaX
            pan.y -= event.scrollingDeltaY
            panChanged()
            presentSurface(force: true, upload: false)
            return
        }
        // Follow the hand for the gesture decision; panning keeps the scroll direction.
        let inverted = event.isDirectionInvertedFromDevice
        gesture.dx += inverted ? -event.scrollingDeltaX : event.scrollingDeltaX
        gesture.dy += inverted ? -event.scrollingDeltaY : event.scrollingDeltaY
        pan.x -= event.scrollingDeltaX
        pan.y -= event.scrollingDeltaY
        panChanged()
        if event.phase.contains(.ended) {
            decideFlick(gesture, at: event)
            return
        }
        flick = gesture
        presentSurface(force: true, upload: false)
    }

    /// Put the canvas back where the gesture started, then run the command the flick asked for.
    /// A slow horizontal drag has already panned and never reaches the speed test.
    func decideFlick(_ gesture: (start: TimeInterval, dx: CGFloat, dy: CGFloat,
                                         pan: CGPoint),
                             at event: NSEvent) {
        flick = nil
        let elapsed = max(event.timestamp - gesture.start, 0.01)
        let speed = abs(gesture.dx) / elapsed
        let horizontal = abs(gesture.dx) >= 90 && abs(gesture.dy) <= abs(gesture.dx) * 0.5
        guard horizontal, elapsed <= 0.5, speed >= 500 else {
            panChanged()
            presentSurface(force: true, upload: false)
            return
        }
        pan = gesture.pan
        panChanged()
        if event.modifierFlags.contains(.option) {
            model?.stepLayer(gesture.dx < 0 ? 1 : -1)
        } else if gesture.dx < 0 {
            model?.undo()
        } else {
            model?.redo()
        }
        swallowMomentum = true
        NSHapticFeedbackManager.defaultPerformer.perform(.alignment, performanceTime: .now)
        presentSurface(force: true)
        overlay.needsDisplay = true
    }

    /// Opt in to AppKit's swipe tracking as well: a system configured for three-finger swipes
    /// still delivers `swipe(with:)`, and both paths must lead to the same command.
    override func wantsScrollEventsForSwipeTracking(on axis: NSEvent.GestureAxis) -> Bool {
        axis == .horizontal
    }

    /// A two-finger swipe on the trackpad, when the system maps swipes to page navigation. Left
    /// goes back, right goes forward; on a painting that is undo and redo. With Option held the
    /// same swipe walks the layer stack, which is the gesture a painter reaches for mid-stroke.
    override func swipe(with event: NSEvent) {
        guard event.deltaX != 0 else {
            super.swipe(with: event)
            return
        }
        if event.modifierFlags.contains(.option) {
            model?.stepLayer(event.deltaX < 0 ? 1 : -1)
            NSHapticFeedbackManager.defaultPerformer.perform(.alignment, performanceTime: .now)
        } else if event.deltaX < 0 {
            model?.undo()
        } else {
            model?.redo()
        }
        presentSurface(force: true)
        overlay.needsDisplay = true
    }

    /// A Force Touch deep click with a still pen opens the few commands a painter needs without
    /// leaving the canvas. Dragging starts a stroke and never fires this, because the deep-click
    /// behaviour is disabled once a drag begins.
    override func pressureChange(with event: NSEvent) {
        guard event.stage == 2, event.stageTransition > 0, !stroking else { return }
        showQuickMenu(at: convert(event.locationInWindow, from: nil))
    }

    func showQuickMenu(at point: CGPoint) {
        guard let model else { return }
        closeQuickMenu()
        NSHapticFeedbackManager.defaultPerformer.perform(.generic, performanceTime: .now)
        let state = QuickMenuState()
        let menu = QuickMenuView(
            model: model,
            state: state,
            tool: tool,
            brush: currentBrush,
            recentColours: recentColours,
            onTool: { [weak self] tool in self?.onTool?(tool) },
            onBrush: { [weak self] brush in self?.onBrush?(brush) },
            onColor: { [weak self] colour in self?.onColor?(colour) },
            onFit: { [weak self] in self?.fit() },
            onClose: { [weak self] in self?.closeQuickMenu() }
        )
        let overlay = QuickMenuOverlay(menu: menu, state: state, point: point,
                                       bounds: bounds.size,
                                       onClose: { [weak self] in self?.closeQuickMenu() })
        let host = NSHostingView(rootView: overlay)
        host.frame = bounds
        host.autoresizingMask = [.width, .height]
        host.wantsLayer = true
        host.alphaValue = 0
        addSubview(host, positioned: .above, relativeTo: self.overlay)
        quickMenuHost = host
        quickMenuState = state
        quickMenuKeyMonitor = NSEvent.addLocalMonitorForEvents(matching: .keyDown) {
            [weak self] event in
            guard event.keyCode == 53 else { return event }
            self?.closeQuickMenu()
            return nil
        }
        NSAnimationContext.runAnimationGroup { context in
            context.duration = 0.14
            host.animator().alphaValue = 1
        }
    }

    func closeQuickMenu() {
        guard let host = quickMenuHost, let state = quickMenuState else { return }
        quickMenuHost = nil
        quickMenuState = nil
        if let monitor = quickMenuKeyMonitor {
            NSEvent.removeMonitor(monitor)
            quickMenuKeyMonitor = nil
        }
        // Let the ring retract and the paper dim clear before the hosting view goes away, so a
        // click outside has the same closing motion as the opening reveal.
        state.visible = false
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.32) {
            host.removeFromSuperview()
        }
    }

    // MARK: Tablet buttons

    override func rightMouseDown(with event: NSEvent) {
        tabletAction(tabletPrimary, event: event)
    }

    override func otherMouseDown(with event: NSEvent) {
        tabletAction(tabletSecondary, event: event)
    }

    override func rightMouseDragged(with event: NSEvent) {
        panWithButton(event)
    }

    override func otherMouseDragged(with event: NSEvent) {
        panWithButton(event)
    }

    override func rightMouseUp(with event: NSEvent) {
        endButtonPan(event)
    }

    override func otherMouseUp(with event: NSEvent) {
        endButtonPan(event)
    }

    func tabletAction(_ action: TabletButtonAction, event: NSEvent) {
        guard let model else { return }
        let point = convert(event.locationInWindow, from: nil)
        switch action {
        case .quickMenu:
            showQuickMenu(at: point)
        case .eyedropper:
            if let pixel = imagePoint(point), let (color, glow) = model.pickInk(at: pixel) {
                onPick?(color, glow)
            }
        case .undo:
            model.undo()
            presentSurface(force: true)
        case .redo:
            model.redo()
            presentSurface(force: true)
        case .brush:
            onTool?(.brush)
        case .eraser:
            onTool?(.eraser)
        case .layerUp:
            model.stepLayer(1)
        case .layerDown:
            model.stepLayer(-1)
        case .pan:
            panButton = event.buttonNumber
            panLastPoint = point
            NSCursor.closedHand.push()
        case .none:
            return
        }
        NSHapticFeedbackManager.defaultPerformer.perform(.alignment, performanceTime: .now)
    }

    func panWithButton(_ event: NSEvent) {
        guard let button = panButton, button == event.buttonNumber,
              let last = panLastPoint else { return }
        let point = convert(event.locationInWindow, from: nil)
        pan.x += point.x - last.x
        pan.y += point.y - last.y
        panLastPoint = point
        presentSurface(force: true, upload: false)
        overlay.needsDisplay = true
    }

    func endButtonPan(_ event: NSEvent) {
        guard panButton == event.buttonNumber else { return }
        panButton = nil
        panLastPoint = nil
        NSCursor.pop()
    }

    func zoom(by factor: CGFloat, around point: CGPoint) {
        stopZoomAnimation()
        guard let model else { return }
        let anchor = imagePoint(point)
        let next = min(max(zoom * factor, 0.1), 32)
        guard abs(next - zoom) > 0.0001 else { return }
        zoom = next
        onZoom?(zoom)
        // Keep the pixel under the cursor under the cursor. Without this, zooming moves the detail
        // the painter is looking at out from under the pen.
        if let anchor {
            let rect = imageRect
            let scale = rect.width / CGFloat(model.width)
            let originX = (bounds.width - CGFloat(model.width) * scale) / 2
            let originY = (bounds.height - CGFloat(model.height) * scale) / 2
            pan.x = point.x - anchor.x * scale - originX
            pan.y = point.y - (CGFloat(model.height) - anchor.y) * scale - originY
            panChanged()
        } else {
            reportViewport()
        }
        presentSurface(force: true, upload: false)
    }

    /// A two-finger double tap is the system's zoom gesture; on a painting it means "show me the
    /// whole sheet again".
    override func smartMagnify(with event: NSEvent) {
        fit()
        NSHapticFeedbackManager.defaultPerformer.perform(.alignment, performanceTime: .now)
    }

    func center(onNormalized point: CGPoint) {
        let size = imageRect.size
        let target = CGPoint(x: imageRect.minX + point.x * size.width,
                             y: imageRect.minY + point.y * size.height)
        pan.x += bounds.midX - target.x
        pan.y += bounds.midY - target.y
        panChanged()
        presentSurface(force: true, upload: false)
        reportViewport()
        overlay.needsDisplay = true
    }

    func fit() {
        pan = .zero
        panChanged()
        animateZoom(to: 1)
        presentSurface(force: true, upload: false)
    }

    /// Zoom from the status bar, around the canvas centre rather than the pointer.
    func setColorSpace(_ space: PaintColorSpace) {
        guard space != colorSpace else { return }
        colorSpace = space
        // The drawable's colour space is part of the surface configuration, so an SDR change
        // needs a new surface just like a switch to the extended path.
        if surface != nil { recreateSurface() }
        overlay.needsDisplay = true
    }

    func setExternalPan(_ value: CGPoint) {
        if abs(value.x - pan.x) > 0.5 || abs(value.y - pan.y) > 0.5 {
            pan = value
            reportViewport()
            overlay.needsDisplay = true
        }
    }

    func panChanged() {
        onPan?(pan)
        reportViewport()
    }

    func setExternalZoom(_ value: CGFloat) {
        let clamped = min(max(value, 0.1), 32)
        if let animation = zoomAnimation {
            // Our own tick reports the same target back through the binding; only a new target
            // from the person retargets the animation.
            if abs(animation.to - clamped) < 0.001 {
                return
            }
        }
        animateZoom(to: clamped)
    }

    /// Zoom is chrome, not direct manipulation, so it settles with an ease-out rather than
    /// snapping. A trackpad pinch cancels it and takes over 1:1.
    func animateZoom(to target: CGFloat) {
        let clamped = min(max(target, 0.1), 32)
        guard abs(clamped - zoom) > 0.001 else { return }
        zoomTimer?.invalidate()
        zoomAnimation = (zoom, clamped, CACurrentMediaTime(), 0.22)
        // The status label follows the destination at once; the canvas catches up.
        onZoom?(clamped)
        let timer = Timer(timeInterval: 1.0 / 60.0, target: self,
                          selector: #selector(stepZoomTimer), userInfo: nil, repeats: true)
        RunLoop.main.add(timer, forMode: .common)
        zoomTimer = timer
    }

    @objc private func stepZoomTimer() {
        stepZoom()
    }

    func stepZoom() {
        guard let animation = zoomAnimation else {
            stopZoomAnimation()
            return
        }
        let elapsed = CACurrentMediaTime() - animation.start
        let t = min(max(elapsed / animation.duration, 0), 1)
        let eased = 1 - pow(1 - t, 3)
        zoom = animation.from + (animation.to - animation.from) * CGFloat(eased)
        reportViewport()
        presentSurface(force: true, upload: false)
        if t >= 1 {
            zoom = animation.to
            stopZoomAnimation()
            presentSurface(force: true, upload: false)
        }
    }

    func stopZoomAnimation() {
        zoomAnimation = nil
        zoomTimer?.invalidate()
        zoomTimer = nil
    }
}
