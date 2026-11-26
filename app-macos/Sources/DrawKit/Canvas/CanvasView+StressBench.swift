// A launch-time painting stress benchmark. It drives the real mouse handlers with random strokes
// and, in mix mode, cycles brushes, eraser, smudge, fill, undo/redo and a new layer, so every path
// a person uses is measured under the same memory and timing probes. Only runs when
// DRAWKIT_BENCH_STRESS is set.

import AppKit
import Darwin
import Foundation
import Metal

extension CanvasView {
    static var stressBenchmarkRequested: Bool {
        ProcessInfo.processInfo.environment["DRAWKIT_BENCH_STRESS"] != nil
    }

    /// One operation per iteration in mix mode.
    private enum StressOp: CaseIterable {
        case ink, pencil, softRound, marker, airbrush, bristle, spray, chalk, watercolour
        case smudge, eraser, fill, undo

        var brush: BrushKind? {
            switch self {
            case .ink: .ink
            case .pencil: .pencil
            case .softRound: .softRound
            case .marker: .marker
            case .airbrush: .airbrush
            case .bristle: .bristle
            case .spray: .spray
            case .chalk: .chalk
            case .watercolour: .watercolor
            default: nil
            }
        }

        var tool: Tool {
            switch self {
            case .smudge: .smudge
            case .eraser: .eraser
            case .fill: .fill
            default: .brush
            }
        }

        var name: String {
            switch self {
            case .softRound: "soft-round"
            case .watercolour: "watercolour"
            case .undo: "undo+redo"
            default: String(describing: self)
            }
        }
    }

    func runStressBenchmark() {
        guard let model, let window else {
            stressReport("no window")
            NSApp.terminate(nil)
            return
        }
        let env = ProcessInfo.processInfo.environment
        let side = Int(env["DRAWKIT_BENCH_SIZE"] ?? "") ?? 3840
        let strokeBudget = Int(env["DRAWKIT_BENCH_STRESS_STROKES"] ?? "") ?? 400
        let secondsBudget = Double(env["DRAWKIT_BENCH_STRESS_SECONDS"] ?? "") ?? 20
        let layerEvery = Int(env["DRAWKIT_BENCH_STRESS_LAYERS"] ?? "") ?? 0
        let mix = env["DRAWKIT_BENCH_STRESS_MIX"] != nil
        let settle = Double(env["DRAWKIT_BENCH_STRESS_SETTLE_MS"] ?? "") ?? 1.0
        let ops = mix ? StressOp.allCases : [.ink]

        NSApp.activate()
        window.makeKeyAndOrderFront(nil)
        model.reset(width: side, height: side)
        model.setBrush(.ink)
        model.setDeferredRaster(true)
        tool = .brush
        createSurface()
        presentSurface(force: true, upload: true)

        let device = MTLCreateSystemDefaultDevice()
        let metalBudget = device?.recommendedMaxWorkingSetSize ?? 0
        let footprintStart = stressFootprint()
        let metalStart = UInt64(device?.currentAllocatedSize ?? 0)
        var footprintPeak = footprintStart
        var metalPeak = metalStart
        var opMs: [String: [Double]] = [:]
        var inStrokePresentMs: [Double] = []
        var presentMs: [Double] = []
        var presentFailures = 0
        var undoNoOps = 0
        var longestStallMs = 0.0
        var previousDone = 0.0
        var done = 0
        var layersAdded = 0
        var opIndex = 0
        var rng = SystemRandomNumberGenerator()
        let runStart = CFAbsoluteTimeGetCurrent()
        // The drag handlers present internally; this separates that cost from the paint itself.
        presentProbe = { inStrokePresentMs.append($0) }
        suppressPresent = env["DRAWKIT_BENCH_STRESS_NOPRESENT"] != nil

        func event(_ type: NSEvent.EventType, at pixel: CGPoint) -> NSEvent? {
            NSEvent.mouseEvent(
                with: type, location: convert(viewPoint(pixel), to: nil), modifierFlags: [],
                timestamp: ProcessInfo.processInfo.systemUptime,
                windowNumber: window.windowNumber, context: nil, eventNumber: 0,
                clickCount: 1, pressure: 1)
        }

        func stroke(_ from: CGPoint, _ to: CGPoint) {
            guard let down = event(.leftMouseDown, at: from) else { return }
            mouseDown(with: down)
            let samples = Int.random(in: 8...24, using: &rng)
            let dx = to.x - from.x
            let dy = to.y - from.y
            let normal = max(hypot(dx, dy), 1)
            let wave = normal * CGFloat.random(in: 0.05...0.12, using: &rng)
                * (Bool.random(using: &rng) ? 1 : -1)
            for step in 1...samples {
                let t = CGFloat(step) / CGFloat(samples)
                let offset = sin(t * 2 * CGFloat.pi) * wave
                let point = CGPoint(x: from.x + dx * t - dy / normal * offset,
                                    y: from.y + dy * t + dx / normal * offset)
                if let drag = event(.leftMouseDragged, at: point) {
                    mouseDragged(with: drag)
                }
            }
            if let up = event(.leftMouseUp, at: to) {
                mouseUp(with: up)
            }
        }

        while done < strokeBudget {
            if CFAbsoluteTimeGetCurrent() - runStart >= secondsBudget { break }
            if layerEvery > 0, done > 0, done % layerEvery == 0 {
                let started = CFAbsoluteTimeGetCurrent()
                model.addLayer()
                opMs["layer add", default: []].append(
                    (CFAbsoluteTimeGetCurrent() - started) * 1000)
                layersAdded += 1
            }
            let op = ops[opIndex % ops.count]
            opIndex += 1
            tool = op.tool
            if let brush = op.brush {
                model.setBrush(brush)
            }
            let size = CGFloat(side)
            let start = CGPoint(x: .random(in: 0..<size, using: &rng),
                                y: .random(in: 0..<size, using: &rng))
            let length = size * .random(in: 0.08...0.30, using: &rng)
            let angle = Double.random(in: 0..<(2 * Double.pi), using: &rng)
            let end = CGPoint(x: min(max(start.x + CGFloat(cos(angle)) * length, 0), size - 1),
                              y: min(max(start.y + CGFloat(sin(angle)) * length, 0), size - 1))
            radius = CGFloat.random(in: 8...48, using: &rng)
            ink = Ink(red: Float.random(in: 0...0.3, using: &rng),
                      green: Float.random(in: 0...0.3, using: &rng),
                      blue: Float.random(in: 0...0.3, using: &rng), alpha: 1)

            let started = CFAbsoluteTimeGetCurrent()
            switch op {
            case .undo:
                let before = model.revision
                model.undo()
                model.redo()
                if model.revision == before { undoNoOps += 1 }
            case .fill:
                if let down = event(.leftMouseDown, at: start) {
                    mouseDown(with: down)
                }
                if let up = event(.leftMouseUp, at: end) {
                    mouseUp(with: up)
                }
            default:
                stroke(start, end)
            }
            if settle > 0 {
                RunLoop.current.run(until: Date(timeIntervalSinceNow: settle / 1000))
            }
            let handlerEnd = CFAbsoluteTimeGetCurrent()
            opMs[op.name, default: []].append((handlerEnd - started) * 1000)

            if !suppressPresent {
                presentSurface(force: true, upload: true)
            }
            let presentEnd = CFAbsoluteTimeGetCurrent()
            presentMs.append((presentEnd - handlerEnd) * 1000)
            if lastPresentCode == 0 { presentFailures += 1 }

            footprintPeak = max(footprintPeak, stressFootprint())
            metalPeak = max(metalPeak, UInt64(device?.currentAllocatedSize ?? 0))
            if done > 0 {
                longestStallMs = max(longestStallMs, (presentEnd - previousDone) * 1000)
            }
            previousDone = presentEnd
            done += 1
        }

        presentProbe = nil
        suppressPresent = false
        let elapsed = CFAbsoluteTimeGetCurrent() - runStart
        let finished = done == strokeBudget
        model.isDirty = false
        model.discardAutosave()

        stressReport(String(format: "config canvas %dx%d strokes %d seconds %.0f mix %@ layers every %d",
                            side, side, strokeBudget, secondsBudget, mix ? "yes" : "no",
                            layerEvery))
        stressReport(String(format: "run ops %d in %.2fs (%.1f/s) %@",
                            done, elapsed, Double(done) / max(elapsed, 0.001),
                            finished ? "finished all" : "hit time budget"))
        for (name, values) in opMs.sorted(by: { $0.key < $1.key }) {
            stressReport(stressPercentiles("op \(name) n=\(values.count)", values))
        }
        stressReport(stressPercentiles("present in stroke", inStrokePresentMs))
        stressReport(stressPercentiles("present after op", presentMs))
        stressReport(String(format: "footprint start %.1f MB peak %.1f MB",
                            Double(footprintStart) / 1_048_576,
                            Double(footprintPeak) / 1_048_576))
        stressReport(String(format: "metal alloc start %.1f MB peak %.1f MB budget %.1f MB",
                            Double(metalStart) / 1_048_576,
                            Double(metalPeak) / 1_048_576,
                            Double(metalBudget) / 1_048_576))
        stressReport(String(format: "stall %.1f ms, layers %d (%d added), undo no-ops %d, present code %d failures %d",
                            longestStallMs, model.layers.count, layersAdded, undoNoOps,
                            lastPresentCode, presentFailures))
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.3) {
            NSApp.terminate(nil)
        }
    }

    /// Resident memory for this process. TASK_VM_INFO carries the same phys_footprint the kernel
    /// charges to the app, and it is cheap enough to read after every operation.
    private func stressFootprint() -> UInt64 {
        var info = task_vm_info_data_t()
        var count = mach_msg_type_number_t(MemoryLayout<task_vm_info_data_t>.size
                                          / MemoryLayout<integer_t>.size)
        let result = withUnsafeMutablePointer(to: &info) { pointer in
            pointer.withMemoryRebound(to: integer_t.self, capacity: Int(count)) {
                task_info(mach_task_self_, task_flavor_t(TASK_VM_INFO), $0, &count)
            }
        }
        return result == KERN_SUCCESS ? info.phys_footprint : 0
    }

    private func stressPercentiles(_ label: String, _ values: [Double]) -> String {
        let sorted = values.sorted()
        guard !sorted.isEmpty else { return "\(label): no samples" }
        let pick = { (fraction: Double) in
            sorted[min(sorted.count - 1, Int((Double(sorted.count - 1) * fraction).rounded()))]
        }
        return String(format: "%@ p50 %.2f ms p95 %.2f ms max %.2f ms", label,
                      pick(0.5), pick(0.95), sorted[sorted.count - 1])
    }

    private func stressReport(_ message: String) {
        FileHandle.standardError.write(Data(("DRAWKIT_STRESS \(message)\n").utf8))
    }
}
