// The replay state machine. The event layout and C entry points live in TapeReplay+Events.

import Foundation

final class TapeReplay {
    let core: UnsafeMutableRawPointer
    private let layerCount: Int
    /// The document size in units, which is what the recorded events use.
    let docWidth: Int
    let docHeight: Int
    /// The size of the compositor and the frames it returns.
    private let width: Int
    private let height: Int
    let spec: OutputSpec
    private let hdr: Bool
    private var handle: UnsafeMutableRawPointer?
    /// The canvas surface a scrubbed frame is presented on, when the replay shares its device.
    private weak var surface: PaintSurface?
    /// The composed frame a fill floods over. Kept across fills: a canvas frame is not small.
    private var frame: [UInt8] = []
    private var events: [TapeEvent] = []
    private var painted: Set<Int> = []
    private var applied = 0
    private var appliedTime: UInt64 = 0
    private var strokeLayer = -1
    private var strokeGroup = UInt64.max
    private var strokeRed: Float = 1
    private var strokeGreen: Float = 1
    private var strokeBlue: Float = 1
    private var strokeFlow: Float = 1
    private var started = false

    /// Null when the tape or the document uses something the compositor cannot reproduce.
    convenience init?(handle core: UnsafeMutableRawPointer, width: Int, height: Int) {
        self.init(handle: core, width: width, height: height,
                  spec: .canvas(width: width, height: height))
    }

    /// A replay whose compositor lives on the canvas surface's device, so a scrubbed frame can be
    /// presented without a readback. `compositor` is null for an export, which creates its own.
    convenience init?(handle core: UnsafeMutableRawPointer, width: Int, height: Int,
                      compositor: UnsafeMutableRawPointer?, surface: PaintSurface) {
        self.init(handle: core, width: width, height: height,
                  spec: .canvas(width: width, height: height),
                  compositor: compositor, surface: surface)
    }

    /// Render the tape at `spec`. The document size is the `width` x `height` the events were
    /// recorded in; the compositor is created at the spec's output size.
    convenience init?(handle core: UnsafeMutableRawPointer, width: Int, height: Int,
                      spec: OutputSpec) {
        self.init(handle: core, width: width, height: height, spec: spec,
                  compositor: nil, surface: nil)
    }

    init?(handle core: UnsafeMutableRawPointer, width: Int, height: Int, spec: OutputSpec,
          compositor: UnsafeMutableRawPointer?, surface: PaintSurface?) {
        guard width > 0, height > 0, spec.width > 0, spec.height > 0 else { return nil }
        // A scaled extended-range render needs float resampling of the base layers; until that
        // exists, the caller keeps the full-size replay.
        if dk_dynamic_range(core) != 0 && spec.pixelsPerUnit != 1 { return nil }
        let count = Int(dk_layer_count(core))
        guard count > 0 else { return nil }

        self.core = core
        layerCount = count
        docWidth = width
        docHeight = height
        self.width = spec.width
        self.height = spec.height
        self.spec = spec
        hdr = dk_dynamic_range(core) != 0
        let events = TapeReplay.loadEvents(core)
        guard let events, !events.isEmpty else { return nil }
        var painted: Set<Int> = []
        for event in events {
            guard event.kind <= 4 else { return nil }
            painted.insert(event.layer)
        }
        self.events = events
        self.painted = painted
        let created = compositor ?? (hdr
            ? dk_gpu_compositor_new_hdr(UInt32(spec.width), UInt32(spec.height))
            : dk_gpu_compositor_new(UInt32(spec.width), UInt32(spec.height)))
        guard let handle = created else {
            return nil
        }
        self.handle = handle
        self.surface = surface
        dk_gpu_compositor_set_budget(handle, 512 << 20)
        // A scrub re-runs thousands of recorded samples at once; one submission per sample would
        // cost more than the coverage it draws.
        dk_gpu_compositor_set_stroke_batch(handle, 1)
        guard resetLayers() else {
            dk_gpu_compositor_free(handle)
            self.handle = nil
            return nil
        }
    }

    deinit {
        dk_gpu_compositor_free(handle)
    }

    /// Render the tape up to `timeMs` as premultiplied RGBA8.
    var isHdr: Bool { hdr }

    /// Apply the tape up to `timeMs` without reading the frame back.
    func apply(to timeMs: UInt64) -> Bool {
        guard let handle else { return false }
        if timeMs < appliedTime && !restart() {
            return false
        }
        apply(upTo: timeMs)
        return true
    }

    /// Whether this replay can present without a readback.
    var sharesSurface: Bool { surface != nil }

    /// Present the current replay frame on the surface that owns the compositor.
    func present(paper: CGRect) -> Bool {
        guard let handle, let surface else { return false }
        return surface.presentReplay(handle, paper: paper) != 0
    }

    func frame(at timeMs: UInt64, into out: UnsafeMutablePointer<UInt8>) -> Bool {
        guard !hdr, let handle else { return false }
        if timeMs < appliedTime && !restart() {
            return false
        }
        apply(upTo: timeMs)
        return dk_gpu_compositor_composite(
            handle, out, UInt32(width), UInt32(height)) != 0
    }

    /// The same frame for an extended-range document, as premultiplied linear floats.
    func frameLinear(at timeMs: UInt64, into out: UnsafeMutablePointer<Float>) -> Bool {
        guard hdr, let handle else { return false }
        if timeMs < appliedTime && !restart() {
            return false
        }
        apply(upTo: timeMs)
        return dk_gpu_compositor_composite_linear(
            handle, out, UInt32(width), UInt32(height)) != 0
    }

    private static func loadEvents(_ core: UnsafeMutableRawPointer) -> [TapeEvent]? {
        let count = Int(dk_replay_event_count(core))
        guard count > 0 else { return nil }
        var events: [TapeEvent] = []
        events.reserveCapacity(count)
        for index in 0..<count {
            var bytes = [UInt8](repeating: 0, count: 128)
            let read = bytes.withUnsafeMutableBytes {
                dk_replay_event(core, UInt32(index), $0.baseAddress)
            }
            guard read != 0, let event = TapeEvent(bytes) else { return nil }
            events.append(event)
        }
        return events
    }

    /// Reset the resident stack to the document before the first stroke. A painted layer starts
    /// empty; every other layer keeps the pixels it already has.
    @discardableResult
    private func resetLayers() -> Bool {
        guard let handle else { return false }
        dk_gpu_compositor_stroke_end(handle)
        started = false
        strokeLayer = -1
        strokeGroup = .max
        applied = 0
        appliedTime = 0
        dk_gpu_compositor_reset(
            handle, UInt32(layerCount), UInt32(width), UInt32(height))
        for index in 0..<layerCount {
            let kind = dk_layer_kind(core, UInt32(index))
            var fill = [Float](repeating: 0, count: 4)
            let isFill = kind == 2 && fill.withUnsafeMutableBufferPointer {
                dk_layer_fill(core, UInt32(index), $0.baseAddress) != 0
            }
            dk_gpu_compositor_set_layer(
                handle, UInt32(index),
                dk_layer_opacity(core, UInt32(index)),
                dk_layer_blend(core, UInt32(index)),
                dk_layer_visible(core, UInt32(index)) != 0 ? 1 : 0,
                isFill ? 1 : 0, fill[0], fill[1], fill[2], fill[3])
            dk_gpu_compositor_set_layer_parent(
                handle, UInt32(index), dk_layer_parent(core, UInt32(index)))
            if dk_layer_clipping(core, UInt32(index)) != 0 {
                dk_gpu_compositor_set_layer_clip(
                    handle, UInt32(index), dk_layer_clip_base(core, UInt32(index)))
            }
            guard kind == 0, !painted.contains(index) else { continue }
            guard uploadBase(index, handle) else { return false }
        }
        return true
    }

    private func restart() -> Bool {
        resetLayers()
    }

    private func apply(upTo timeMs: UInt64) {
        guard let handle else { return }
        while applied < events.count, events[applied].timeMs <= timeMs {
            let event = events[applied]
            if event.kind == 3 {
                closeStroke(handle)
                smudge(event, handle)
                applied += 1
                continue
            }
            if event.kind == 2 {
                closeStroke(handle)
                fill(event, handle)
                applied += 1
                continue
            }
            if event.kind == 4 {
                closeStroke(handle)
                clear(event, handle)
                applied += 1
                continue
            }
            if !started || event.layer != strokeLayer || event.group != strokeGroup {
                closeStroke(handle)
                beginStroke(event, handle)
            }
            segment(event, handle)
            applied += 1
        }
        appliedTime = timeMs
    }

    /// One recorded fill: the mask comes from the core's flood pipeline over the composed frame,
    /// and the write lands in the layer's tile backups.
    private func fill(_ event: TapeEvent, _ handle: UnsafeMutableRawPointer) {
        _ = dk_gpu_compositor_materialize(handle)
        if frame.count != width * height * 4 {
            frame = [UInt8](repeating: 0, count: width * height * 4)
        }
        let composed = frame.withUnsafeMutableBufferPointer {
            dk_gpu_compositor_composite(handle, $0.baseAddress, UInt32(width), UInt32(height))
        }
        guard composed != 0 else { return }
        var mask = [Float](repeating: 0, count: width * height)
        let seedX = UInt32((Float(event.seedX) * Float(spec.pixelsPerUnit)).rounded())
        let seedY = UInt32((Float(event.seedY) * Float(spec.pixelsPerUnit)).rounded())
        // A fill on transparent paper goes under the line; one on existing paint replaces it.
        let under = dk_gpu_compositor_layer_alpha(
            handle, UInt32(event.layer), seedX, seedY) < 0.5
        let built = frame.withUnsafeBufferPointer { source in
            mask.withUnsafeMutableBufferPointer { target in
                dk_fill_mask(
                    source.baseAddress, UInt32(width), UInt32(height),
                    seedX, seedY, event.tolerance,
                    spec.pixels(event.close), spec.pixels(event.expand),
                    spec.pixels(event.feather),
                    target.baseAddress, UInt32(target.count),
                    event.smart != 0 ? 1 : 0, under ? 1 : 0)
            }
        }
        guard built != 0 else { return }
        let locked = dk_layer_alpha_locked(core, UInt32(event.layer)) != 0
        let colour = linearColour(event.red, event.green, event.blue)
        _ = mask.withUnsafeBufferPointer {
            dk_gpu_compositor_apply_fill(
                handle, UInt32(event.layer), $0.baseAddress,
                UInt32(width), UInt32(height),
                colour.0, colour.1, colour.2, event.alpha, locked ? 1 : 0,
                under ? 1 : 0)
        }
    }

    /// One recorded smudge sample. It has to commit before the next sample, because the next one
    /// pulls the paint this one moved.
    private func smudge(_ event: TapeEvent, _ handle: UnsafeMutableRawPointer) {
        // The core refuses to smudge an alpha-locked layer, so the replay must too.
        if dk_layer_alpha_locked(core, UInt32(event.layer)) != 0 {
            return
        }
        let kind = UInt32(max(0, event.brush))
        func param(_ id: UInt32) -> Float {
            dk_brush_slot_param(core, kind, id)
        }
        let pressure = min(max(event.pressure, 0), 1)
        let pressureRadius = param(2)
        let radius = spec.radius(event.radius, recorded: .canvas)
            * (pressureRadius + (1 - pressureRadius) * pressure)
        let from = spec.point(event.fromX, event.fromY)
        let to = spec.point(event.toX, event.toY)
        dk_gpu_compositor_stroke_begin(
            handle, UInt32(width), UInt32(height), 0, 0, 0, 1,
            event.strength, 0, 0, 0,
            0, 1, 0, 1, 0, 0, 0,
            0, 0, 0, 0, 0,
            0, 0, 0)
        dk_gpu_compositor_stroke_segment(
            handle, from.0, from.1, to.0, to.1,
            radius, param(0), 0.25, 1, 0, 1)
        dk_gpu_compositor_set_stroke_smudge(
            handle, from.0 - to.0, from.1 - to.1)
        _ = dk_gpu_compositor_stroke_commit(handle, UInt32(event.layer), 0, 0, 0, 1)
        dk_gpu_compositor_stroke_end(handle)
        dk_gpu_compositor_clear_history(handle)
    }

    /// A recorded clear fills the layer with one colour, which is a solid rectangle upload.
    private func clear(_ event: TapeEvent, _ handle: UnsafeMutableRawPointer) {
        if hdr {
            let colour = linearColour(event.red, event.green, event.blue)
            var solid = [Float](repeating: 0, count: width * height * 4)
            for at in stride(from: 0, to: solid.count, by: 4) {
                solid[at] = colour.0
                solid[at + 1] = colour.1
                solid[at + 2] = colour.2
                solid[at + 3] = min(max(event.alpha, 0), 1)
            }
            _ = solid.withUnsafeBufferPointer {
                dk_gpu_compositor_upload_layer_linear(
                    handle, UInt32(event.layer), UInt32(width), UInt32(height),
                    $0.baseAddress)
            }
            return
        }
        var solid = [UInt8](repeating: 0, count: width * height * 4)
        let red = UInt8((min(max(event.red, 0), 1) * 255).rounded())
        let green = UInt8((min(max(event.green, 0), 1) * 255).rounded())
        let blue = UInt8((min(max(event.blue, 0), 1) * 255).rounded())
        let alpha = UInt8((min(max(event.alpha, 0), 1) * 255).rounded())
        for at in stride(from: 0, to: solid.count, by: 4) {
            solid[at] = red
            solid[at + 1] = green
            solid[at + 2] = blue
            solid[at + 3] = alpha
        }
        _ = solid.withUnsafeBufferPointer {
            dk_gpu_compositor_upload_layer(
                handle, UInt32(event.layer), 0, 0, UInt32(width), UInt32(height),
                $0.baseAddress)
        }
    }

    private func linearColour(_ red: Float, _ green: Float, _ blue: Float) -> (Float, Float, Float) {
        guard hdr else { return (red, green, blue) }
        var values = [Float](repeating: 0, count: 3)
        let converted = values.withUnsafeMutableBufferPointer {
            dk_linear_colour(core, red, green, blue, $0.baseAddress)
        }
        guard converted != 0 else { return (red, green, blue) }
        return (values[0], values[1], values[2])
    }

    private func closeStroke(_ handle: UnsafeMutableRawPointer) {
        guard started else { return }
        dk_gpu_compositor_set_stroke_alpha_lock(
            handle, dk_layer_alpha_locked(core, UInt32(strokeLayer)) != 0 ? 1 : 0)
        _ = dk_gpu_compositor_stroke_commit(
            handle, UInt32(strokeLayer), strokeRed, strokeGreen, strokeBlue, strokeFlow)
        dk_gpu_compositor_stroke_end(handle)
        dk_gpu_compositor_clear_history(handle)
        started = false
    }

    private func beginStroke(_ event: TapeEvent, _ handle: UnsafeMutableRawPointer) {
        let kind = UInt32(max(0, event.brush))
        func param(_ id: UInt32) -> Float {
            dk_brush_slot_param(core, kind, id)
        }
        let erase = event.kind == 1
        let dualKind = Int32(param(14).rounded())
        let colour = linearColour(event.red, event.green, event.blue)
        let red = erase ? 1 : colour.0
        let green = erase ? 1 : colour.1
        let blue = erase ? 1 : colour.2
        let alpha = erase ? 1 : event.alpha
        dk_gpu_compositor_stroke_begin(
            handle, UInt32(width), UInt32(height),
            red, green, blue, alpha,
            param(1), param(3), param(9), param(10),
            param(12), param(13),
            dk_brush_kind_param(dualKind, 0),
            dk_brush_kind_param(dualKind, 7),
            dk_brush_kind_param(dualKind, 6),
            dk_brush_kind_param(dualKind, 9),
            dk_brush_kind_param(dualKind, 10),
            param(11), param(5), param(8),
            dk_brush_kind_param(dualKind, 5),
            dk_brush_kind_param(dualKind, 8),
            UInt32(truncatingIfNeeded: event.seed),
            UInt32(truncatingIfNeeded: event.seed >> 32),
            erase ? 1 : 0)
        strokeLayer = event.layer
        strokeGroup = event.group
        strokeRed = red
        strokeGreen = green
        strokeBlue = blue
        strokeFlow = param(1)
        started = true
    }

    private func segment(_ event: TapeEvent, _ handle: UnsafeMutableRawPointer) {
        let kind = UInt32(max(0, event.brush))
        let hardness = dk_brush_slot_param(core, kind, 0)
        let pressureRadius = dk_brush_slot_param(core, kind, 2)
        let spacing = dk_brush_slot_param(core, kind, 4)
        var roundness = dk_brush_slot_param(core, kind, 7)
        var angle = dk_brush_slot_param(core, kind, 6)
        // The brush follows the pen: tilt squashes the oval and turns its major axis. This is the
        // same shaping `Brush::with_input` does for the core.
        let raw = (event.tiltX * event.tiltX + event.tiltY * event.tiltY).squareRoot()
        if raw > 0.001 || abs(event.roll) > 0.001 {
            let strength = raw <= 1 ? raw : min(raw / 60, 1)
            let direction = raw > 0.01
                ? atan2(event.tiltY, event.tiltX)
                : event.azimuth
            roundness = max(roundness * (1 - 0.85 * strength), 0.08)
            angle += direction * 180 / .pi + event.roll * 180 / .pi
        }
        let pressure = min(max(event.pressure, 0), 1)
        let recorded = BrushScale(rawValue: Int(event.scale)) ?? .canvas
        let radius = spec.radius(event.radius, recorded: recorded)
            * (pressureRadius + (1 - pressureRadius) * pressure)
        let from = spec.point(event.fromX, event.fromY)
        let to = spec.point(event.toX, event.toY)
        dk_gpu_compositor_stroke_segment(
            handle, from.0, from.1, to.0, to.1,
            radius, hardness, spacing, roundness, angle, pressure)
    }
}
