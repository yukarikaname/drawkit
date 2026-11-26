// A launch-time pointer benchmark. It feeds the real mouse handlers synthetic events, so the
// stroke, the tile commit, the materialise roundtrip and the present follow exactly the path a
// person exercises. Two strokes are drawn: the second one is what used to overwrite the first
// with the CPU cache. Only runs when DRAWKIT_BENCH_POINTER is set.

import AppKit
import Foundation

extension CanvasView {
    static var pointerBenchmarkRequested: Bool {
        ProcessInfo.processInfo.environment["DRAWKIT_BENCH_POINTER"] != nil
    }

    func runPointerBenchmark() {
        guard let model, let window else {
            pointerReport("no window")
            finishPointerBenchmark()
            return
        }
        let side = Int(ProcessInfo.processInfo.environment["DRAWKIT_BENCH_SIZE"] ?? "") ?? 2048
        let hdr = ProcessInfo.processInfo.environment["DRAWKIT_BENCH_HDR"] != nil
        NSApp.activate()
        window.makeKeyAndOrderFront(nil)
        model.reset(width: side, height: side, colorSpace: hdr ? .displayP3 : .sRGB, hdr: hdr)
        model.setBrush(.ink)
        model.setDeferredRaster(true)
        if hdr {
            extended = true
            recreateSurface()
        }
        createSurface()
        presentSurface(force: true, upload: true)

        let start = CGPoint(x: Double(side) * 0.35, y: Double(side) * 0.4)
        let end = CGPoint(x: Double(side) * 0.65, y: Double(side) * 0.4)
        ink = Ink(red: 0.05, green: 0.05, blue: 0.05, alpha: 1)
        radius = 18

        func event(_ type: NSEvent.EventType, at point: CGPoint) -> NSEvent? {
            NSEvent.mouseEvent(
                with: type, location: convert(point, to: nil), modifierFlags: [],
                timestamp: ProcessInfo.processInfo.systemUptime,
                windowNumber: window.windowNumber, context: nil, eventNumber: 0,
                clickCount: 1, pressure: 1)
        }
        func stroke(_ from: CGPoint, _ to: CGPoint) {
            guard let down = event(.leftMouseDown, at: viewPoint(from)) else { return }
            mouseDown(with: down)
            for step in 1...12 {
                let t = CGFloat(step) / 12
                let point = CGPoint(x: from.x + (to.x - from.x) * t, y: from.y)
                guard let drag = event(.leftMouseDragged, at: viewPoint(point)) else { continue }
                mouseDragged(with: drag)
            }
            if let up = event(.leftMouseUp, at: viewPoint(to)) {
                mouseUp(with: up)
            }
        }

        let middle = CGPoint(x: (start.x + end.x) / 2, y: start.y)
        // Inside the first commit's rectangle but off the painted line.
        let probes = [start, middle, end, CGPoint(x: start.x + 40, y: start.y - 14)]
        func snapshot() -> String {
            probes.map { pointerPick(model, at: $0) }.joined(separator: " | ")
        }

        pointerReport("surface \(surface != nil) layers \(model.layers.count)"
            + " gpu \(model.gpuCanvas) extended \(surfaceExtended)")
        stroke(start, end)
        let before = snapshot()
        pointerReport("first \(before) present \(lastPresentCode)")

        // The second stroke starts far from the first, so any damage to the first one comes from
        // the materialise and upload of its committed rectangle, not from new paint.
        stroke(CGPoint(x: start.x, y: start.y + 300),
               CGPoint(x: end.x, y: end.y + 300))
        let after = snapshot()
        pointerReport("second \(after) present \(lastPresentCode)"
            + (before == after ? " previous intact" : " previous CHANGED"))

        // The real GPU commit path: materialise the tiles, recolour through the tape, and undo.
        let group = model.strokeGroup(at: start)
        if group != 0 {
            let red = Ink(red: 1, green: 0, blue: 0, alpha: 1)
            let changed = model.recolorStroke(group, ink: red)
            presentSurface(force: true, upload: true)
            pointerReport("recolor \(changed ? "ok" : "refused")"
                + " \(pointerPick(model, at: start))")
            model.undo()
            presentSurface(force: true, upload: true)
            pointerReport("recolor undone \(pointerPick(model, at: start))")
        } else {
            pointerReport("recolor: no stroke under the start")
        }

        // Auto Shape must paint the ideal stroke in the same pointer-up, not on the next one.
        model.setAutoShape(true)
        tool = .brush
        let shapeStart = CGPoint(x: start.x, y: start.y + 600)
        let shapeEnd = CGPoint(x: end.x, y: end.y + 600)
        stroke(shapeStart, shapeEnd)
        let shapeMiddle = CGPoint(x: (shapeStart.x + shapeEnd.x) / 2, y: shapeStart.y)
        pointerReport("shape immediate \(pointerPick(model, at: shapeMiddle))"
            + " start \(pointerPick(model, at: CGPoint(x: shapeStart.x + 4, y: shapeStart.y)))"
            + " end \(pointerPick(model, at: CGPoint(x: shapeEnd.x - 4, y: shapeEnd.y)))")
        model.setAutoShape(false)
        sampleLatency(model, start: CGPoint(x: start.x, y: start.y + 900),
                      end: CGPoint(x: end.x, y: end.y + 900))
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.4) { [weak self] in
            guard let self, let model = self.model else { return }
            self.pointerReport("settled \(self.pointerPick(model, at: start))"
                + " deferred \(model.deferredPending)")
            self.finishPointerBenchmark()
        }
    }

    /// What one pen sample costs on the main thread, with and without the frame it produces. The
    /// sample handler paints and then presents; a slow present is time the next sample waits.
    private func sampleLatency(_ model: PaintModel, start: CGPoint, end: CGPoint) {
        func run(present: Bool) -> (costs: [Double], frames: Int) {
            suppressPresent = !present
            let ink = self.ink
            guard let down = NSEvent.mouseEvent(
                with: .leftMouseDown, location: convert(viewPoint(start), to: nil),
                modifierFlags: [], timestamp: ProcessInfo.processInfo.systemUptime,
                windowNumber: window?.windowNumber ?? 0, context: nil, eventNumber: 0,
                clickCount: 1, pressure: 1) else { return ([], 0) }
            mouseDown(with: down)
            var costs: [Double] = []
            let framesBefore = self.presentCount
            for step in 1...48 {
                let t = CGFloat(step) / 48
                let point = CGPoint(x: start.x + (end.x - start.x) * t, y: start.y)
                guard let drag = NSEvent.mouseEvent(
                    with: .leftMouseDragged, location: convert(viewPoint(point), to: nil),
                    modifierFlags: [], timestamp: ProcessInfo.processInfo.systemUptime,
                    windowNumber: window?.windowNumber ?? 0, context: nil, eventNumber: 0,
                    clickCount: 1, pressure: 1) else { continue }
                let before = CFAbsoluteTimeGetCurrent()
                mouseDragged(with: drag)
                costs.append((CFAbsoluteTimeGetCurrent() - before) * 1000)
            }
            if let up = NSEvent.mouseEvent(
                with: .leftMouseUp, location: convert(viewPoint(end), to: nil),
                modifierFlags: [], timestamp: ProcessInfo.processInfo.systemUptime,
                windowNumber: window?.windowNumber ?? 0, context: nil, eventNumber: 0,
                clickCount: 1, pressure: 1) {
                mouseUp(with: up)
            }
            suppressPresent = false
            _ = ink
            return (costs, self.presentCount - framesBefore)
        }
        let samplesPerStroke = 48
        func stats(_ costs: [Double]) -> String {
            guard !costs.isEmpty else { return "none" }
            let sorted = costs.sorted()
            let p50 = sorted[sorted.count / 2]
            let p95 = sorted[min(sorted.count - 1, Int(Double(sorted.count) * 0.95))]
            return String(format: "p50 %.2f ms p95 %.2f ms max %.2f ms",
                          p50, p95, sorted[sorted.count - 1])
        }
        // Warm the first stroke's tile path so the numbers describe the steady state.
        _ = run(present: true)
        model.undo()
        let painted = run(present: true)
        model.undo()
        let bare = run(present: false)
        model.undo()
        pointerReport("latency present \(stats(painted.costs)) frames \(painted.frames)"
            + " of \(samplesPerStroke) samples | paint only \(stats(bare.costs))")
    }

    private func pointerPick(_ model: PaintModel, at point: CGPoint) -> String {
        if let pixel = surface?.pick(x: Int(point.x), y: Int(point.y)) {
            return String(format: "%.2f,%.2f,%.2f", pixel.0, pixel.1, pixel.2)
        }
        // An extended-range stack has no eight-bit readback; the CPU composite is the check.
        if let color = model.pick(at: point),
           let rgb = NSColor(color).usingColorSpace(.deviceRGB) {
            return String(format: "%.2f,%.2f,%.2f",
                          rgb.redComponent, rgb.greenComponent, rgb.blueComponent)
        }
        return "nil"
    }

    /// Leave no dirty document behind: a benchmark quits without a save prompt or a recovery copy.
    private func finishPointerBenchmark() {
        model?.isDirty = false
        model?.discardAutosave()
        NSApp.terminate(nil)
    }

    private func pointerReport(_ message: String) {
        FileHandle.standardError.write(Data(("DRAWKIT_POINTER \(message)\n").utf8))
    }
}
