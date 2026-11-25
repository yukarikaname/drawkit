// A launch-time render benchmark for the resolution-independent path. It paints one stroke with
// each thickness policy and replays both at half size, then measures the painted thickness in the
// output. Only runs when DRAWKIT_BENCH_RENDER is set.

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
        model.reset(width: side, height: side)
        model.setBrush(.ink)
        model.setDeferredRaster(true)
        let ink = Ink(red: 0, green: 0, blue: 0, alpha: 1)
        paint(model, y: 250, scale: .canvas, ink: ink)
        paint(model, y: 550, scale: .screen, ink: ink)
        model.setDeferredRaster(false)
        model.isDirty = false
        model.discardAutosave()

        for (name, thickness) in [("recorded", RenderThickness.recorded),
                                  ("screen", .screen), ("canvas", .canvas)] {
            let spec = OutputSpec.scaled(width: side, height: side, percent: 50,
                                         thickness: thickness)
            guard let pixels = model.renderTape(at: spec) else {
                renderReport("\(name): render failed")
                continue
            }
            let canvasRow = thicknessOf(pixels, spec: spec, at: 125)
            let screenRow = thicknessOf(pixels, spec: spec, at: 275)
            renderReport("\(name) canvas-drawn \(canvasRow) px, screen-drawn \(screenRow) px")
        }
        NSApp.terminate(nil)
    }

    private func paint(_ model: PaintModel, y: CGFloat, scale: BrushScale, ink: Ink) {
        model.brushScale = scale
        model.beginStroke()
        model.stroke(from: CGPoint(x: 200, y: y), to: CGPoint(x: 600, y: y),
                     radius: 20, pressure: 1, ink: ink)
        model.endStroke()
    }

    /// The painted height of the horizontal stroke around `row` in the output.
    private func thicknessOf(_ pixels: [UInt8], spec: OutputSpec, at row: Int) -> Int {
        let column = spec.width / 2
        let low = max(0, row - 40)
        let high = min(spec.height - 1, row + 40)
        var first = -1
        var last = -1
        for y in low...high {
            let alpha = pixels[(y * spec.width + column) * 4 + 3]
            if alpha > 32 {
                if first < 0 { first = y }
                last = y
            }
        }
        return first < 0 ? 0 : last - first + 1
    }

    private func renderReport(_ message: String) {
        FileHandle.standardError.write(Data(("DRAWKIT_RENDER \(message)\n").utf8))
    }
}
