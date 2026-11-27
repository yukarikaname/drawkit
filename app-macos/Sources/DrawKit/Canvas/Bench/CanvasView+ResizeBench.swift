// A launch-time check for live resizing. It drives the window through a series of content sizes and
// times what a resize step costs the main thread: the layout pass, the surface resize, and the
// frame that follows. Only runs when DRAWKIT_BENCH_RESIZE is set.

import AppKit
import Foundation

extension CanvasView {
    static var resizeBenchmarkRequested: Bool {
        ProcessInfo.processInfo.environment["DRAWKIT_BENCH_RESIZE"] != nil
    }

    func runResizeBenchmark() {
        guard let model, let window else {
            resizeReport("no window")
            NSApp.terminate(nil)
            return
        }
        let env = ProcessInfo.processInfo.environment
        let side = Int(env["DRAWKIT_BENCH_RESIZE_SIZE"] ?? "") ?? 4096
        let steps = Int(env["DRAWKIT_BENCH_RESIZE_STEPS"] ?? "") ?? 60
        let warmup = Int(env["DRAWKIT_BENCH_RESIZE_WARMUP"] ?? "") ?? 5
        model.reset(width: side, height: side)
        model.setBrush(.ink)
        tool = .brush
        createSurface()
        // Resizing an empty document only measures the paper quad. Paint first, so the present has
        // resident tiles to composite and the numbers describe a real painting.
        let ink = Ink(red: 0, green: 0, blue: 0, alpha: 1)
        let edge = CGFloat(side)
        for row in 0..<8 {
            let y = edge / 8 * CGFloat(row + 1)
            model.beginStroke()
            model.stroke(
                from: CGPoint(x: 200, y: y), to: CGPoint(x: edge - 200, y: y + 40),
                radius: 24, pressure: 1, ink: ink)
            model.endStroke()
        }
        presentSurface(force: true, upload: true)
        guard model.paintSurface?.supportsLayers == true else {
            resizeReport("no resident layers")
            NSApp.terminate(nil)
            return
        }
        resizeReport("config canvas \(side)x\(side) steps \(steps) warmup \(warmup)")

        func resizeOnce(_ step: Int) -> (layout: Double, present: Double) {
            // A drag sweeps the size continuously; alternate the axes so both directions run.
            let width = 1180 + (step * 23) % 420
            let height = 700 + (step * 17) % 260
            let start = CFAbsoluteTimeGetCurrent()
            window.setContentSize(NSSize(width: width, height: height))
            window.contentView?.layoutSubtreeIfNeeded()
            let layout = (CFAbsoluteTimeGetCurrent() - start) * 1000
            // What the coalesced turn does next: one present at the new size.
            let presentStart = CFAbsoluteTimeGetCurrent()
            presentSurface(force: true, upload: false)
            return (layout, (CFAbsoluteTimeGetCurrent() - presentStart) * 1000)
        }

        // The first steps allocate drawables at the new sizes; they are not a steady-state drag.
        for step in 0..<warmup {
            _ = resizeOnce(step)
        }
        var samples: [Double] = []
        var layouts: [Double] = []
        var presents = 0
        for step in warmup..<(warmup + steps) {
            let (layout, present) = resizeOnce(step)
            presents += 1
            layouts.append(layout)
            samples.append(layout + present)
        }
        samples.sort()
        layouts.sort()
        let median = samples[samples.count / 2]
        let p95 = samples[min(samples.count - 1, Int(Double(samples.count) * 0.95))]
        resizeReport(
            String(
                format: "resize step p50 %.2f ms p95 %.2f ms max %.2f ms,"
                    + " layout p50 %.2f ms, presents %d",
                median, p95, samples.last ?? 0,
                layouts[layouts.count / 2], presents))
        NSApp.terminate(nil)
    }

    func resizeReport(_ line: String) {
        FileHandle.standardError.write(Data("DRAWKIT_RESIZE \(line)\n".utf8))
    }
}
