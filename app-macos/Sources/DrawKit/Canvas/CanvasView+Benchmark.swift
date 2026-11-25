// A launch-time stroke benchmark for the real macOS path: the core stroke, the dirty upload and
// the Metal present, with no synthetic mouse events and no welcome sheet. It only runs when
// DRAWKIT_BENCH is set, prints one line to stderr and quits.

import AppKit
import CoreGraphics
import Foundation

extension CanvasView {
    static var benchmarkRequested: Bool {
        ProcessInfo.processInfo.environment["DRAWKIT_BENCH"] != nil
    }

    func scheduleBenchmarkIfRequested() {
        guard Self.benchmarkRequested else { return }
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.6) { [weak self] in
            self?.runStrokeBenchmark()
        }
    }

    private func runStrokeBenchmark() {
        guard let model else {
            report("no model")
            NSApp.terminate(nil)
            return
        }
        let canvas = 3840
        let gpuPreview = ProcessInfo.processInfo.environment["DRAWKIT_BENCH_CPU"] == nil
        model.reset(width: canvas, height: canvas)
        model.setBrush(.ink)
        let radius: CGFloat = 24
        let ink = Ink(red: 0.08, green: 0.07, blue: 0.06, alpha: 1)
        var strokeMs: [Double] = []
        var presentMs: [Double] = []
        if gpuPreview {
            model.setDeferredRaster(true)
        }
        model.beginStroke()
        if gpuPreview {
            surface?.strokeBegin(width: canvas, height: canvas,
                                 color: (ink.red, ink.green, ink.blue, ink.alpha))
        }
        for index in 0..<240 {
            let x = 320.0 + Double(index % 160) * 18.0
            let y = 1_600.0 + sin(Double(index) * 0.07) * 260.0
            let start = CFAbsoluteTimeGetCurrent()
            model.stroke(from: CGPoint(x: x, y: y),
                         to: CGPoint(x: x + 14, y: y + 3),
                         radius: radius, pressure: 0.8, ink: ink)
            if gpuPreview {
                surface?.strokeSegment(
                    fromX: Float(x), fromY: Float(y),
                    toX: Float(x + 14), toY: Float(y + 3),
                    radius: Float(radius), hardness: Float(model.brushParameters[0]),
                    spacing: Float(model.brushParameters[4]),
                    roundness: Float(model.brushParameters[7]),
                    angle: Float(model.brushParameters[6]))
            }
            let afterStroke = CFAbsoluteTimeGetCurrent()
            presentSurface(force: true, upload: true)
            let afterPresent = CFAbsoluteTimeGetCurrent()
            strokeMs.append((afterStroke - start) * 1000)
            presentMs.append((afterPresent - afterStroke) * 1000)
        }
        let commitStart = CFAbsoluteTimeGetCurrent()
        model.endStroke()
        let commitMs = (CFAbsoluteTimeGetCurrent() - commitStart) * 1000
        if gpuPreview {
            model.setDeferredRaster(false)
            surface?.strokeEnd()
        }
        model.isDirty = false
        model.discardAutosave()

        let path = gpuPreview ? "gpu-preview" : "cpu"
        report("canvas \(canvas)x\(canvas) \(path) \(strokeMs.count) samples")
        report(percentiles("stroke", strokeMs))
        report(percentiles("present", presentMs))
        report(String(format: "commit %.2f ms", commitMs))
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.3) {
            NSApp.terminate(nil)
        }
    }

    private func percentiles(_ label: String, _ values: [Double]) -> String {
        let sorted = values.sorted()
        guard !sorted.isEmpty else { return "\(label): no samples" }
        let pick = { (fraction: Double) in
            sorted[min(sorted.count - 1, Int((Double(sorted.count - 1) * fraction).rounded()))]
        }
        let average = sorted.reduce(0, +) / Double(sorted.count)
        return String(format: "%@ avg %.2f ms p50 %.2f p95 %.2f p99 %.2f max %.2f",
                      label, average, pick(0.5), pick(0.95), pick(0.99), sorted.last ?? 0)
    }

    private func report(_ message: String) {
        FileHandle.standardError.write(Data(("DRAWKIT_BENCH \(message)\n").utf8))
    }
}
