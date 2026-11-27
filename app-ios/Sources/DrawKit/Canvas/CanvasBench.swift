// A pointer benchmark for the iPad shell, mirroring `DRAWKIT_BENCH_POINTER` on the Mac: paint one
// stroke through the same calls the touch handler makes and log what a sample costs. It runs on
// the first layout and only when the environment says so, which `devicectl` can set:
//
//   xcrun devicectl device process launch --device <udid> \
//       --environment DRAWKIT_BENCH_POINTER=1 dev.drawkit.ipados
//
// The numbers land in the device console next to the app's other output. `Studio` is main-actor
// isolated, so the bench is too: the caller is a layout pass.

import Foundation
import UIKit

@MainActor
enum CanvasBench {
    static var requested: Bool {
        // `devicectl` passes `DEVICECTL_CHILD_…` into the process environment; a launch argument
        // (`-DRAWKIT_BENCH_POINTER 1`) lands in the argument domain of UserDefaults, which is the
        // other way to ask a launched app for a run.
        ProcessInfo.processInfo.environment["DRAWKIT_BENCH_POINTER"] == "1"
            || UserDefaults.standard.bool(forKey: "DRAWKIT_BENCH_POINTER")
    }

    static func runIfRequested(studio: Studio, canvas: CGSize) {
        guard requested else { return }
        Task { @MainActor in
            try? await Task.sleep(for: .milliseconds(800))
            run(studio: studio, canvas: canvas)
        }
    }

    private static func run(studio: Studio, canvas: CGSize) {
        let side = CGFloat(studio.width)
        guard side > 0 else { return }
        let y = side * 0.4
        let start = CGPoint(x: side * 0.35, y: y)
        let end = CGPoint(x: side * 0.65, y: y)

        var samples: [Double] = []
        studio.beginStroke()
        var last = start
        for step in 1...48 {
            let t = CGFloat(step) / 48
            let point = CGPoint(x: start.x + (end.x - start.x) * t, y: y)
            let began = CFAbsoluteTimeGetCurrent()
            studio.segment(from: last, to: point, pressure: 1, tilt: [0, 0], azimuth: 0, roll: 0)
            samples.append((CFAbsoluteTimeGetCurrent() - began) * 1000)
            last = point
        }
        studio.endStroke()
        samples.sort()
        let p50 = samples[samples.count / 2]
        let p95 = samples[min(samples.count - 1, samples.count * 95 / 100)]
        let ink = studio.pick(at: CGPoint(x: start.x + 50, y: y))
        let report = String(
            format:
                "canvas %dx%d view %.0fx%.0f pen sample p50 %.2f ms p95 %.2f ms max %.2f ms ink %@",
            studio.width, studio.height, canvas.width, canvas.height,
            p50, p95, samples.last ?? 0, ink ? "painted" : "empty")
        NSLog("DRAWKIT_IOS_POINTER %@", report)
        // `devicectl --console` shows stdout, which is where a launched app can be read back.
        print("DRAWKIT_IOS_POINTER \(report)")
    }
}
