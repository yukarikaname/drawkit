// A launch-time render benchmark for the resolution-independent path. It paints one stroke at the
// document size and replays it at half size under both thickness policies, then measures the
// painted thickness in the output. Only runs when DRAWKIT_BENCH_RENDER is set.

import AppKit
import Foundation

extension CanvasView {
    static var renderBenchmarkRequested: Bool {
        ProcessInfo.processInfo.environment["DRAWKIT_BENCH_RENDER"] != nil
    }

    func runRenderBenchmark() {
        guard let model else {
            renderReport("no model")
            NSApp.terminate(nil)
            return
        }
        let side = 800
        let radius: CGFloat = 20
        model.reset(width: side, height: side)
        model.setBrush(.ink)
        model.setDeferredRaster(true)
        let ink = Ink(red: 0, green: 0, blue: 0, alpha: 1)
        model.beginStroke()
        model.stroke(from: CGPoint(x: 200, y: 400), to: CGPoint(x: 600, y: 400),
                     radius: radius, pressure: 1, ink: ink)
        model.endStroke()
        model.setDeferredRaster(false)
        model.isDirty = false
        model.discardAutosave()

        for (name, policy) in [("screen", BrushScale.screen), ("canvas", BrushScale.canvas)] {
            let spec = OutputSpec.scaled(width: side, height: side, percent: 50, brush: policy)
            guard let pixels = model.renderTape(at: spec) else {
                renderReport("\(name): render failed")
                continue
            }
            let column = spec.width / 2
            var thickness = 0
            for y in 0..<spec.height {
                let alpha = pixels[(y * spec.width + column) * 4 + 3]
                if alpha > 32 {
                    thickness += 1
                }
            }
            renderReport("\(name) \(spec.width)×\(spec.height) thickness \(thickness) px")
        }
        NSApp.terminate(nil)
    }

    private func renderReport(_ message: String) {
        FileHandle.standardError.write(Data(("DRAWKIT_RENDER \(message)\n").utf8))
    }
}
