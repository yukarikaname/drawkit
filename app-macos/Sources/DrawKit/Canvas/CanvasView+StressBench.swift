// A launch-time painting stress benchmark. It drives the real mouse handlers with random strokes
// and, in mix mode, cycles brushes, eraser, smudge, fill, undo/redo and the layer, transform and
// clipboard operations the palette and Edit menu use, so every path a person uses is measured
// under the same memory and timing probes. Only runs when DRAWKIT_BENCH_STRESS is set.

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
        case layerAdd, layerRemove, layerMove, group, reparent
        case layerOpacity, layerBlend, visibility, transform, copy, paste

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
            case .layerAdd: "layer-add"
            case .layerRemove: "layer-remove"
            case .layerMove: "layer-move"
            case .layerOpacity: "layer-opacity"
            case .layerBlend: "layer-blend"
            case .copy: "copy-merged"
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
        let baseSide = Int(env["DRAWKIT_BENCH_SIZE"] ?? "") ?? 3840
        var side = baseSide
        let strokeBudget = Int(env["DRAWKIT_BENCH_STRESS_STROKES"] ?? "") ?? 400
        let secondsBudget = Double(env["DRAWKIT_BENCH_STRESS_SECONDS"] ?? "") ?? 20
        let layerEvery = Int(env["DRAWKIT_BENCH_STRESS_LAYERS"] ?? "") ?? 0
        let mix = env["DRAWKIT_BENCH_STRESS_MIX"] != nil
        let baseLayers = Int(env["DRAWKIT_BENCH_STRESS_BASE_LAYERS"] ?? "") ?? 0
        let fixedRadius = CGFloat(Double(env["DRAWKIT_BENCH_STRESS_RADIUS"] ?? "") ?? 0)
        let settle = Double(env["DRAWKIT_BENCH_STRESS_SETTLE_MS"] ?? "") ?? 1.0
        let ops = mix ? StressOp.allCases : [.ink]
        // Keep a layer stack bounded: a pasted 4K layer carries a full canvas of tiles.
        let layerCap = min(24, max(6, 402_653_184 / max(baseSide * baseSide * 4, 1)))

        NSApp.activate()
        window.makeKeyAndOrderFront(nil)
        model.reset(width: baseSide, height: baseSide)
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
        var opFootprint: [String: [Double]] = [:]
        var opMetal: [String: [Double]] = [:]
        var inStrokePresentMs: [Double] = []
        var presentMs: [Double] = []
        var presentFailures = 0
        var undoNoOps = 0
        var copyFailures = 0
        var pasteFailures = 0
        var longestStallMs = 0.0
        var previousDone = 0.0
        var done = 0
        var layersAdded = 0
        var layersRemoved = 0
        var layersMoved = 0
        var blendIndex = 0
        var opacityLow = false
        var reparentIn = false
        var opIndex = 0
        var rng = SystemRandomNumberGenerator()
        // The drag handlers present internally; this separates that cost from the paint itself.
        presentProbe = { inStrokePresentMs.append($0) }
        suppressPresent = env["DRAWKIT_BENCH_STRESS_NOPRESENT"] != nil

        func paintIndex() -> Int? {
            model.layers.firstIndex { $0.kind == .paint }
        }

        func keepPaintableActive() {
            guard let index = paintIndex(), model.activeLayer != index else { return }
            model.selectLayer(index)
        }

        func ensureLayers(_ minimum: Int) {
            while model.layers.count < minimum {
                model.addLayer()
                layersAdded += 1
            }
            keepPaintableActive()
        }

        func trimLayers() {
            while model.layers.count > layerCap {
                let before = model.layers.count
                if let group = model.layers.last(where: { $0.kind == .group }) {
                    model.selectLayer(group.id)
                }
                model.removeLayer()
                let after = model.layers.count
                layersRemoved += max(0, before - after)
                if after == before { break }
            }
            keepPaintableActive()
        }

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

        var setupMs = 0.0
        if baseLayers > 1 {
            let setupStart = CFAbsoluteTimeGetCurrent()
            let hostPresent = suppressPresent
            suppressPresent = true
            for layer in 1..<baseLayers {
                model.addLayer()
                let y = CGFloat(side) * (CGFloat(layer) + 0.5) / CGFloat(baseLayers)
                radius = 18 + CGFloat(layer % 4) * 8
                ink = Ink(red: Float(layer % 5) * 0.16, green: 0.22, blue: 0.5, alpha: 0.85)
                if env["DRAWKIT_BENCH_STRESS_BASE_EMPTY"] == nil {
                    let reach = env["DRAWKIT_BENCH_STRESS_BASE_SHORT"] == nil ? 0.92 : 0.12
                    stroke(CGPoint(x: CGFloat(side) * 0.08, y: y),
                           CGPoint(x: CGFloat(side) * reach, y: y + 10))
                    // A real session yields between strokes; let the deferred materialise and the
                    // debounced thumbnails run instead of piling tile copies up.
                    if settle > 0 {
                        RunLoop.current.run(until: Date(timeIntervalSinceNow: settle / 1000))
                    }
                }
                if env["DRAWKIT_BENCH_STRESS_BASE_TRACE"] != nil {
                    stressReport(String(format: "base %d footprint %.0f MB metal %.0f MB",
                                        model.layers.count,
                                        Double(stressFootprint()) / 1_048_576,
                                        Double(device?.currentAllocatedSize ?? 0) / 1_048_576))
                }
            }
            suppressPresent = hostPresent
            model.selectLayer(max(0, model.layers.count - 1))
            setupMs = (CFAbsoluteTimeGetCurrent() - setupStart) * 1000
        }
        let runStart = CFAbsoluteTimeGetCurrent()

        while done < strokeBudget {
            if CFAbsoluteTimeGetCurrent() - runStart >= secondsBudget { break }
            if layerEvery > 0, done > 0, done % layerEvery == 0 {
                let started = CFAbsoluteTimeGetCurrent()
                model.addLayer()
                layersAdded += 1
                trimLayers()
                opMs["layer-add", default: []].append(
                    (CFAbsoluteTimeGetCurrent() - started) * 1000)
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
            radius = fixedRadius > 0 ? fixedRadius : CGFloat.random(in: 8...48, using: &rng)
            ink = Ink(red: Float.random(in: 0...0.3, using: &rng),
                      green: Float.random(in: 0...0.3, using: &rng),
                      blue: Float.random(in: 0...0.3, using: &rng), alpha: 1)

            let memBefore = stressFootprint()
            let metalBefore = UInt64(device?.currentAllocatedSize ?? 0)
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
            case .layerAdd:
                model.addLayer()
                layersAdded += 1
                trimLayers()
            case .layerRemove:
                ensureLayers(2)
                let before = model.layers.count
                model.removeLayer()
                layersRemoved += max(0, before - model.layers.count)
                keepPaintableActive()
            case .layerMove:
                ensureLayers(2)
                let subject = model.layers.first { layer in
                    layer.kind == .paint && model.layers.contains {
                        $0.parent == layer.parent && $0.id != layer.id
                    }
                } ?? model.layers[model.activeLayer]
                let up = model.layers.contains {
                    $0.parent == subject.parent && $0.id > subject.id
                }
                model.moveLayer(subject.id, steps: up ? 1 : -1)
                layersMoved += 1
                keepPaintableActive()
            case .group:
                model.addGroup()
                layersAdded += 1
                keepPaintableActive()
                trimLayers()
            case .reparent:
                ensureLayers(2)
                reparentIn.toggle()
                if reparentIn {
                    if !model.layers.contains(where: { $0.kind == .group }) {
                        model.addGroup()
                        layersAdded += 1
                        keepPaintableActive()
                        trimLayers()
                    }
                    if let subject = model.layers.first(where: { $0.kind == .paint }),
                       let group = model.layers.last(where: { $0.kind == .group }),
                       model.reparentLayer(subject.id, parent: group.id) != nil {
                        layersMoved += 1
                    }
                } else if let subject = model.layers.last(where: {
                    $0.kind == .paint && $0.parent != nil
                }), model.reparentLayer(subject.id, parent: nil) != nil {
                    layersMoved += 1
                }
                keepPaintableActive()
            case .layerOpacity:
                keepPaintableActive()
                opacityLow.toggle()
                model.setOpacity(index: model.activeLayer, opacityLow ? 0.45 : 1.0)
            case .layerBlend:
                keepPaintableActive()
                let modes = BlendMode.allCases
                let mode = modes[blendIndex % modes.count]
                blendIndex += 1
                model.setBlend(index: model.activeLayer, mode)
            case .visibility:
                let target = model.layers.last {
                    $0.kind == .paint && $0.id != model.activeLayer
                } ?? model.layers.first
                if let target {
                    model.setVisible(index: target.id, visible: !target.visible)
                }
            case .transform:
                let target = model.width == baseSide ? max(baseSide - 64, 64) : baseSide
                // Resize asks for a present; the no-present run keeps its premise.
                let request = model.requestPresent
                model.requestPresent = nil
                _ = model.resizeCanvas(to: target, height: target, anchorX: 0, anchorY: 0)
                model.requestPresent = request
                side = min(model.width, baseSide)
                keepPaintableActive()
            case .copy:
                if !model.copyPixels(merged: true) { copyFailures += 1 }
            case .paste:
                if !model.pastePixels() { pasteFailures += 1 }
                trimLayers()
            default:
                stroke(start, end)
            }
            if settle > 0 {
                RunLoop.current.run(until: Date(timeIntervalSinceNow: settle / 1000))
            }
            let handlerEnd = CFAbsoluteTimeGetCurrent()
            let memAfter = stressFootprint()
            let metalAfter = UInt64(device?.currentAllocatedSize ?? 0)
            opMs[op.name, default: []].append((handlerEnd - started) * 1000)
            opFootprint[op.name, default: []].append(
                (Double(memAfter) - Double(memBefore)) / 1_048_576)
            opMetal[op.name, default: []].append(
                (Double(metalAfter) - Double(metalBefore)) / 1_048_576)

            if !suppressPresent {
                presentSurface(force: true, upload: true)
            }
            let presentEnd = CFAbsoluteTimeGetCurrent()
            presentMs.append((presentEnd - handlerEnd) * 1000)
            if lastPresentCode == 0 { presentFailures += 1 }

            footprintPeak = max(footprintPeak, max(memAfter, stressFootprint()))
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

        stressReport(String(format: "config canvas %dx%d strokes %d seconds %.0f mix %@ layers every %d base %d",
                            baseSide, baseSide, strokeBudget, secondsBudget, mix ? "yes" : "no",
                            layerEvery, baseLayers))
        if baseLayers > 1 {
            stressReport(String(format: "setup %d painted layers in %.0f ms", baseLayers, setupMs))
        }
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
        stressReport(String(format: "stall %.1f ms, layers %d (%d added, %d removed, %d moved), undo no-ops %d, copy/paste failures %d/%d, present code %d failures %d",
                            longestStallMs, model.layers.count, layersAdded, layersRemoved,
                            layersMoved, undoNoOps, copyFailures, pasteFailures,
                            lastPresentCode, presentFailures))
        stressMemoryReport("mem", opFootprint)
        stressMemoryReport("metal", opMetal)
        if env["DRAWKIT_BENCH_STRESS_HOLD"] != nil {
            stressReport("holding pid \(ProcessInfo.processInfo.processIdentifier)")
            return
        }
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

    /// The three operations that on average added the most bytes, so a growing path stands out
    /// from a stroke that only churns tiles.
    private func stressMemoryReport(_ label: String, _ deltas: [String: [Double]]) {
        let gains = deltas.compactMap { entry -> (String, Double, Double, Int)? in
            let values = entry.value
            guard !values.isEmpty else { return nil }
            let average = values.reduce(0, +) / Double(values.count)
            guard average > 0 else { return nil }
            return (entry.key, average, values.max() ?? 0, values.count)
        }.sorted { $0.1 > $1.1 }.prefix(3)
        for gain in gains {
            stressReport(String(format: "%@ op %@: %+.1f MB avg over %d ops (peak %+.1f)",
                                label, gain.0, gain.1, gain.3, gain.2))
        }
        if gains.isEmpty {
            stressReport("\(label) op: no positive average")
        }
    }

    private func stressReport(_ message: String) {
        FileHandle.standardError.write(Data(("DRAWKIT_STRESS \(message)\n").utf8))
    }
}
