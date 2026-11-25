// Replaying the tape on the headless GPU compositor.
//
// The core still knows how to replay a stroke with the CPU brush, but the frames the timeline shows
// and the timelapse encodes are just a picture: the compositor keeps the layers resident, the
// recorded samples feed the same stroke shader the live canvas uses, and every frame is one
// composite and readback. Events the shader cannot reproduce yet (fill, smudge, clear) make the
// driver unavailable, and the caller keeps the core replay.

import Foundation

@_silgen_name("dk_replay_event_count")
func dk_replay_event_count(_ handle: UnsafeMutableRawPointer?) -> UInt32
@_silgen_name("dk_replay_event")
func dk_replay_event(
    _ handle: UnsafeMutableRawPointer?, _ index: UInt32,
    _ out: UnsafeMutableRawPointer?) -> Int32
@_silgen_name("dk_brush_slot_param")
func dk_brush_slot_param(_ handle: UnsafeMutableRawPointer?, _ kind: UInt32, _ id: UInt32) -> Float
@_silgen_name("dk_gpu_compositor_new")
func dk_gpu_compositor_new(_ width: UInt32, _ height: UInt32) -> UnsafeMutableRawPointer?
@_silgen_name("dk_gpu_compositor_free")
func dk_gpu_compositor_free(_ handle: UnsafeMutableRawPointer?)
@_silgen_name("dk_gpu_compositor_reset")
func dk_gpu_compositor_reset(
    _ handle: UnsafeMutableRawPointer?, _ count: UInt32, _ width: UInt32, _ height: UInt32)
@_silgen_name("dk_gpu_compositor_set_budget")
func dk_gpu_compositor_set_budget(_ handle: UnsafeMutableRawPointer?, _ bytes: UInt64)
@_silgen_name("dk_gpu_compositor_upload_layer")
func dk_gpu_compositor_upload_layer(
    _ handle: UnsafeMutableRawPointer?, _ index: UInt32,
    _ x: UInt32, _ y: UInt32, _ width: UInt32, _ height: UInt32,
    _ pixels: UnsafePointer<UInt8>?) -> Int32
@_silgen_name("dk_gpu_compositor_set_layer")
func dk_gpu_compositor_set_layer(
    _ handle: UnsafeMutableRawPointer?, _ index: UInt32, _ opacity: Float,
    _ blend: Int32, _ visible: Int32, _ fill: Int32,
    _ red: Float, _ green: Float, _ blue: Float, _ alpha: Float)
@_silgen_name("dk_gpu_compositor_composite")
func dk_gpu_compositor_composite(
    _ handle: UnsafeMutableRawPointer?, _ out: UnsafeMutablePointer<UInt8>?,
    _ width: UInt32, _ height: UInt32) -> Int32
@_silgen_name("dk_gpu_compositor_clear_history")
func dk_gpu_compositor_clear_history(_ handle: UnsafeMutableRawPointer?)
@_silgen_name("dk_gpu_compositor_stroke_begin")
func dk_gpu_compositor_stroke_begin(
    _ handle: UnsafeMutableRawPointer?, _ width: UInt32, _ height: UInt32,
    _ red: Float, _ green: Float, _ blue: Float, _ alpha: Float,
    _ flow: Float, _ pressureAlpha: Float, _ paper: Float, _ tip: Float,
    _ dual: Float, _ dualScale: Float, _ dualHardness: Float, _ dualRoundness: Float,
    _ dualAngle: Float, _ dualPaper: Float, _ dualTip: Float,
    _ wet: Float, _ jitter: Float, _ grain: Float,
    _ dualJitter: Float, _ dualGrain: Float,
    _ seedLow: UInt32, _ seedHigh: UInt32, _ erase: Int32)
@_silgen_name("dk_gpu_compositor_stroke_segment")
func dk_gpu_compositor_stroke_segment(
    _ handle: UnsafeMutableRawPointer?,
    _ fromX: Float, _ fromY: Float, _ toX: Float, _ toY: Float,
    _ radius: Float, _ hardness: Float, _ spacing: Float,
    _ roundness: Float, _ angle: Float, _ pressure: Float)
@_silgen_name("dk_gpu_compositor_stroke_end")
func dk_gpu_compositor_stroke_end(_ handle: UnsafeMutableRawPointer?)
@_silgen_name("dk_gpu_compositor_stroke_commit")
func dk_gpu_compositor_stroke_commit(
    _ handle: UnsafeMutableRawPointer?, _ layer: UInt32,
    _ red: Float, _ green: Float, _ blue: Float, _ alpha: Float) -> Int32

/// One event from `dk_replay_event`, decoded from the fixed C layout.
private struct TapeEvent {
    let timeMs: UInt64
    let group: UInt64
    let seed: UInt64
    let kind: UInt32
    let layer: Int
    let brush: Int
    let fromX: Float
    let fromY: Float
    let toX: Float
    let toY: Float
    let radius: Float
    let pressure: Float
    let red: Float
    let green: Float
    let blue: Float
    let alpha: Float
    let tiltX: Float
    let tiltY: Float
    let roll: Float
    let azimuth: Float

    init?(_ bytes: [UInt8]) {
        guard bytes.count >= 120 else { return nil }
        func value<T>(_ offset: Int, _ type: T.Type) -> T {
            bytes.withUnsafeBytes { $0.loadUnaligned(fromByteOffset: offset, as: T.self) }
        }
        timeMs = value(0, UInt64.self)
        group = value(8, UInt64.self)
        seed = value(16, UInt64.self)
        kind = value(24, UInt32.self)
        layer = Int(value(28, UInt32.self))
        brush = Int(value(32, UInt32.self))
        fromX = value(36, Float.self)
        fromY = value(40, Float.self)
        toX = value(44, Float.self)
        toY = value(48, Float.self)
        radius = value(52, Float.self)
        pressure = value(56, Float.self)
        red = value(60, Float.self)
        green = value(64, Float.self)
        blue = value(68, Float.self)
        alpha = value(72, Float.self)
        tiltX = value(76, Float.self)
        tiltY = value(80, Float.self)
        roll = value(84, Float.self)
        azimuth = value(88, Float.self)
    }
}

@MainActor
final class TapeReplay {
    private unowned let model: PaintModel
    private let core: UnsafeMutableRawPointer
    private let layerCount: Int
    private let width: Int
    private let height: Int
    nonisolated(unsafe) private var handle: UnsafeMutableRawPointer?
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
    init?(model: PaintModel) {
        guard let core = model.handle, !model.hdr, !model.layers.isEmpty else { return nil }
        for layer in model.layers where layer.kind == .group || layer.clipping
            || layer.alphaLocked {
            return nil
        }
        self.model = model
        self.core = core
        layerCount = model.layers.count
        width = model.width
        height = model.height
        let events = TapeReplay.loadEvents(core)
        guard let events, !events.isEmpty else { return nil }
        var painted: Set<Int> = []
        for event in events {
            // Fill, smudge and clear have no GPU path yet; the core replay still handles them.
            guard event.kind <= 1 else { return nil }
            painted.insert(event.layer)
        }
        self.events = events
        self.painted = painted
        guard let handle = dk_gpu_compositor_new(UInt32(width), UInt32(height)) else {
            return nil
        }
        self.handle = handle
        dk_gpu_compositor_set_budget(handle, 512 << 20)
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
    func frame(at timeMs: UInt64, into out: UnsafeMutablePointer<UInt8>) -> Bool {
        guard let handle, model.layers.count == layerCount else { return false }
        if timeMs < appliedTime && !restart() {
            return false
        }
        apply(upTo: timeMs)
        return dk_gpu_compositor_composite(
            handle, out, UInt32(width), UInt32(height)) != 0
    }

    private static func loadEvents(_ core: UnsafeMutableRawPointer) -> [TapeEvent]? {
        let count = Int(dk_replay_event_count(core))
        guard count > 0 else { return nil }
        var events: [TapeEvent] = []
        events.reserveCapacity(count)
        for index in 0..<count {
            var bytes = [UInt8](repeating: 0, count: 120)
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
            guard let state = model.gpuLayerState(index: index) else { return false }
            dk_gpu_compositor_set_layer(
                handle, UInt32(index), state.opacity, state.blend,
                state.visible ? 1 : 0, state.fill == nil ? 0 : 1,
                state.fill?.0 ?? 0, state.fill?.1 ?? 0,
                state.fill?.2 ?? 0, state.fill?.3 ?? 0)
            guard model.layers[index].kind == .paint, !painted.contains(index) else {
                continue
            }
            let uploaded = model.withLayerPixels(
                index: index, x: 0, y: 0, width: width, height: height
            ) { pixels in
                dk_gpu_compositor_upload_layer(
                    handle, UInt32(index), 0, 0, UInt32(width), UInt32(height),
                    pixels) != 0
            }
            if !uploaded { return false }
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
            if !started || event.layer != strokeLayer || event.group != strokeGroup {
                closeStroke(handle)
                beginStroke(event, handle)
            }
            segment(event, handle)
            applied += 1
        }
        appliedTime = timeMs
    }

    private func closeStroke(_ handle: UnsafeMutableRawPointer) {
        guard started else { return }
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
        let red = erase ? 1 : event.red
        let green = erase ? 1 : event.green
        let blue = erase ? 1 : event.blue
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
        let radius = event.radius * (pressureRadius + (1 - pressureRadius) * pressure)
        dk_gpu_compositor_stroke_segment(
            handle, event.fromX, event.fromY, event.toX, event.toY,
            radius, hardness, spacing, roundness, angle, pressure)
    }
}
