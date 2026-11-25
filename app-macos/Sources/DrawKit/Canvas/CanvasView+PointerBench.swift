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
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.4) { [weak self] in
            guard let self, let model = self.model else { return }
            self.pointerReport("settled \(self.pointerPick(model, at: start))"
                + " deferred \(model.deferredPending)")
            self.finishPointerBenchmark()
        }
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
