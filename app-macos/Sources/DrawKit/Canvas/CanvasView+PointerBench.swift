// A launch-time pointer benchmark. It feeds the real mouse handlers synthetic events, so the
// stroke, the tile commit and the present follow exactly the path a person exercises. Only runs
// when DRAWKIT_BENCH_POINTER is set.

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

        pointerReport("surface \(surface != nil) layers \(model.layers.count)"
            + " gpu \(model.gpuCanvas)")
        pointerReport("before pick \(pointerPick(model, at: start))")
        guard let down = event(.leftMouseDown, at: viewPoint(start)) else {
            pointerReport("no mouse down")
            finishPointerBenchmark()
            return
        }
        mouseDown(with: down)
        for step in 1...12 {
            let t = CGFloat(step) / 12
            let point = CGPoint(x: start.x + (end.x - start.x) * t, y: start.y)
            guard let drag = event(.leftMouseDragged, at: viewPoint(point)) else { continue }
            mouseDragged(with: drag)
            pointerReport("drag \(step) pick \(pointerPick(model, at: point))"
                + " present \(lastPresentCode) gpuStroke \(gpuStroke)")
        }
        if let up = event(.leftMouseUp, at: viewPoint(end)) {
            mouseUp(with: up)
        }
        pointerReport("after up pick \(pointerPick(model, at: start))"
            + " gpuStroke \(gpuStroke) deferred \(model.deferredPending)"
            + " stale \(model.gpuStale) present \(lastPresentCode)")
        presentSurface(force: true, upload: true)
        let middle = CGPoint(x: (start.x + end.x) / 2, y: start.y)
        pointerReport("line start \(pointerPick(model, at: start))"
            + " mid \(pointerPick(model, at: middle))"
            + " end \(pointerPick(model, at: end))")
        pointerReport("scan \(pointerScan(side: side))")
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.6) { [weak self] in
            guard let self, let model = self.model else { return }
            self.pointerReport("settled pick \(self.pointerPick(model, at: start))"
                + " deferred \(model.deferredPending) present \(self.lastPresentCode)")
            finishPointerBenchmark()
        }
    }

    private func pointerPick(_ model: PaintModel, at point: CGPoint) -> String {
        let direct = surface?.pick(x: Int(point.x), y: Int(point.y))
        let text: String
        if let pixel = direct {
            text = String(format: "%.2f,%.2f,%.2f", pixel.0, pixel.1, pixel.2)
        } else {
            text = "nil"
        }
        var cpu = "nil"
        if let color = model.pick(at: point),
           let rgb = NSColor(color).usingColorSpace(.deviceRGB) {
            cpu = String(format: "%.2f,%.2f,%.2f",
                         rgb.redComponent, rgb.greenComponent, rgb.blueComponent)
        }
        return "surface \(text) model \(cpu)"
    }

    private func pointerScan(side: Int) -> String {
        guard let surface else { return "no surface" }
        var dark = 0
        var box: (Int, Int, Int, Int)?
        let step = max(side / 32, 1)
        for y in stride(from: step / 2, to: side, by: step) {
            for x in stride(from: step / 2, to: side, by: step) {
                guard let pixel = surface.pick(x: x, y: y) else { continue }
                if pixel.0 < 0.5 || pixel.1 < 0.5 || pixel.2 < 0.5 {
                    dark += 1
                    if let current = box {
                        box = (min(current.0, x), min(current.1, y),
                               max(current.2, x), max(current.3, y))
                    } else {
                        box = (x, y, x, y)
                    }
                }
            }
        }
        guard let box else { return "no ink" }
        return "\(dark) dark box \(box.0),\(box.1)-\(box.2),\(box.3)"
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
