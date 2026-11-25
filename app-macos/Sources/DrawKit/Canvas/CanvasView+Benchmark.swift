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
        let canvas = Int(ProcessInfo.processInfo.environment["DRAWKIT_BENCH_SIZE"] ?? "") ?? 3840
        NSApp.activate()
        window?.makeKeyAndOrderFront(nil)
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
            let beginStart = CFAbsoluteTimeGetCurrent()
            surface?.strokeBegin(width: canvas, height: canvas,
                                 color: (ink.red, ink.green, ink.blue, ink.alpha),
                                 flow: Float(model.brushParameters[1]),
                                 pressureAlpha: Float(model.brushParameters[3]),
                                 paper: Float(model.brushParameters[9]),
                                 tip: Float(model.brushParameters[10]),
                                 dual: 0, dualScale: 0, dualHardness: 0,
                                 dualRoundness: 1, dualAngle: 0, dualPaper: 0, dualTip: 0,
                                 wet: 0, jitter: 0, grain: 0, dualJitter: 0,
                                 dualGrain: 0, seed: 0)
            report(String(format: "stroke begin %.2f ms",
                          (CFAbsoluteTimeGetCurrent() - beginStart) * 1000))
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
                    angle: Float(model.brushParameters[6]),
                    pressure: 0.8)
            }
            let afterStroke = CFAbsoluteTimeGetCurrent()
            presentSurface(force: true, upload: true)
            let afterPresent = CFAbsoluteTimeGetCurrent()
            strokeMs.append((afterStroke - start) * 1000)
            presentMs.append((afterPresent - afterStroke) * 1000)
        }
        if gpuPreview, let surface,
           ProcessInfo.processInfo.environment["DRAWKIT_BENCH_TILES"] != nil {
            model.paintSurface = surface
            let start = CFAbsoluteTimeGetCurrent()
            model.endStroke()
            let prepared = CFAbsoluteTimeGetCurrent()
            var applied = false
            var commitMs = 0.0
            var materializeMs = 0.0
            if model.beginTiles() {
                let count = surface.strokeCommit(
                    layer: model.activeLayer,
                    color: (ink.red, ink.green, ink.blue),
                    flow: Float(model.brushParameters[1]))
                var rects: [UInt32] = []
                for index in 0..<max(count, 0) {
                    guard let rect = surface.committedRect(index) else { continue }
                    rects += [UInt32(rect.x), UInt32(rect.y),
                              UInt32(rect.width), UInt32(rect.height)]
                }
                applied = model.endGPUTiles(rects: rects)
                commitMs = (CFAbsoluteTimeGetCurrent() - prepared) * 1000
                let probe = CGPoint(x: 330, y: 1_600)
                let before = model.pick(at: probe).map { NSColor($0).alphaComponent } ?? -1
                let empty = model.pick(at: CGPoint(x: 10, y: 10))
                    .map { NSColor($0).alphaComponent } ?? -1
                report(String(format: "deferred %@ empty alpha %.2f",
                              model.deferredPending ? "yes" : "no", empty))
                let materializeStart = CFAbsoluteTimeGetCurrent()
                model.materializeCanvas()
                materializeMs = (CFAbsoluteTimeGetCurrent() - materializeStart) * 1000
                let after = model.pick(at: probe).map { NSColor($0).alphaComponent } ?? -1
                report(String(format: "cpu cache alpha before %.2f after %.2f", before, after))
            }
            model.setDeferredRaster(false)
            surface.strokeEnd()
            report(String(format: "tile commit %@ prepare %.2f commit %.2f materialize %.2f ms",
                          applied ? "ok" : "refused", (prepared - start) * 1000,
                          commitMs, materializeMs))
            model.isDirty = false
            model.discardAutosave()
            DispatchQueue.main.asyncAfter(deadline: .now() + 0.3) {
                NSApp.terminate(nil)
            }
            return
        }
        if gpuPreview,
           ProcessInfo.processInfo.environment["DRAWKIT_BENCH_PENUP"] != nil {
            // Exercise the real pen-up path: the timer on the run loop advances the commit.
            gpuStroke = true
            let start = CFAbsoluteTimeGetCurrent()
            model.endStroke()
            let prepared = CFAbsoluteTimeGetCurrent()
            startGPUCommit()
            DispatchQueue.main.asyncAfter(deadline: .now() + 0.5) { [weak self] in
                self?.report(String(format: "pen-up prepare %.2f ms, pending %@",
                                    (prepared - start) * 1000,
                                    model.deferredPending ? "yes" : "no"))
                NSApp.terminate(nil)
            }
            return
        }
        let commitStart = CFAbsoluteTimeGetCurrent()
        model.endStroke()
        var maxChunk = 0.0
        var chunks = 0
        if gpuPreview {
            while model.deferredPending {
                let stepStart = CFAbsoluteTimeGetCurrent()
                _ = model.advanceDeferred(commands: 8)
                maxChunk = max(maxChunk, (CFAbsoluteTimeGetCurrent() - stepStart) * 1000)
                chunks += 1
                if !model.deferredPending { break }
            }
        }
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
        report(String(format: "commit %.2f ms in %d chunks, max %.2f ms",
                      commitMs, chunks, maxChunk))
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
