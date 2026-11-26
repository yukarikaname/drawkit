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
@_silgen_name("dk_gpu_compositor_new_hdr")
func dk_gpu_compositor_new_hdr(_ width: UInt32, _ height: UInt32) -> UnsafeMutableRawPointer?
@_silgen_name("dk_gpu_compositor_upload_layer_linear")
func dk_gpu_compositor_upload_layer_linear(
    _ handle: UnsafeMutableRawPointer?, _ index: UInt32,
    _ width: UInt32, _ height: UInt32, _ pixels: UnsafePointer<Float>?) -> Int32
@_silgen_name("dk_gpu_compositor_composite_linear")
func dk_gpu_compositor_composite_linear(
    _ handle: UnsafeMutableRawPointer?, _ out: UnsafeMutablePointer<Float>?,
    _ width: UInt32, _ height: UInt32) -> Int32
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
@_silgen_name("dk_gpu_compositor_set_layer_parent")
func dk_gpu_compositor_set_layer_parent(
    _ handle: UnsafeMutableRawPointer?, _ index: UInt32, _ parent: Int32)
@_silgen_name("dk_gpu_compositor_set_layer_clip")
func dk_gpu_compositor_set_layer_clip(
    _ handle: UnsafeMutableRawPointer?, _ index: UInt32, _ clip: Int32)
@_silgen_name("dk_gpu_compositor_set_layer")
func dk_gpu_compositor_set_layer(
    _ handle: UnsafeMutableRawPointer?, _ index: UInt32, _ opacity: Float,
    _ blend: Int32, _ visible: Int32, _ fill: Int32,
    _ red: Float, _ green: Float, _ blue: Float, _ alpha: Float)
@_silgen_name("dk_fill_mask")
func dk_fill_mask(
    _ composite: UnsafePointer<UInt8>?, _ width: UInt32, _ height: UInt32,
    _ seedX: UInt32, _ seedY: UInt32, _ tolerance: Float,
    _ close: UInt32, _ expand: UInt32, _ feather: UInt32,
    _ out: UnsafeMutablePointer<Float>?, _ len: UInt32, _ smart: Int32,
    _ under: Int32) -> Int32
@_silgen_name("dk_gpu_compositor_apply_fill")
func dk_gpu_compositor_apply_fill(
    _ handle: UnsafeMutableRawPointer?, _ layer: UInt32,
    _ mask: UnsafePointer<Float>?, _ width: UInt32, _ height: UInt32,
    _ red: Float, _ green: Float, _ blue: Float, _ alpha: Float,
    _ alphaLocked: Int32, _ under: Int32) -> Int32
@_silgen_name("dk_gpu_compositor_layer_alpha")
func dk_gpu_compositor_layer_alpha(
    _ handle: UnsafeMutableRawPointer?, _ layer: UInt32, _ x: UInt32, _ y: UInt32) -> Float
@_silgen_name("dk_gpu_compositor_materialize")
func dk_gpu_compositor_materialize(_ handle: UnsafeMutableRawPointer?) -> Int32
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
@_silgen_name("dk_gpu_compositor_set_stroke_alpha_lock")
func dk_gpu_compositor_set_stroke_alpha_lock(
    _ handle: UnsafeMutableRawPointer?, _ locked: Int32)
@_silgen_name("dk_gpu_compositor_set_stroke_smudge")
func dk_gpu_compositor_set_stroke_smudge(
    _ handle: UnsafeMutableRawPointer?, _ dx: Float, _ dy: Float)
@_silgen_name("dk_gpu_compositor_set_stroke_batch")
func dk_gpu_compositor_set_stroke_batch(_ handle: UnsafeMutableRawPointer?, _ on: Int32)
@_silgen_name("dk_gpu_compositor_stroke_end")
func dk_gpu_compositor_stroke_end(_ handle: UnsafeMutableRawPointer?)
@_silgen_name("dk_gpu_compositor_stroke_commit")
func dk_gpu_compositor_stroke_commit(
    _ handle: UnsafeMutableRawPointer?, _ layer: UInt32,
    _ red: Float, _ green: Float, _ blue: Float, _ alpha: Float) -> Int32

/// One event from `dk_replay_event`, decoded from the fixed C layout.
struct TapeEvent {
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
    let tolerance: Float
    let strength: Float
    let seedX: UInt32
    let seedY: UInt32
    let close: UInt32
    let expand: UInt32
    let feather: UInt32
    /// 0 canvas radius, 1 screen radius; zero for events that are not strokes.
    let scale: UInt32
    /// A fill that also used the edge model to close gaps.
    let smart: UInt32

    init?(_ bytes: [UInt8]) {
        guard bytes.count >= 128 else { return nil }
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
        tolerance = value(92, Float.self)
        strength = value(96, Float.self)
        seedX = value(100, UInt32.self)
        seedY = value(104, UInt32.self)
        close = value(108, UInt32.self)
        expand = value(112, UInt32.self)
        feather = value(116, UInt32.self)
        scale = value(120, UInt32.self)
        smart = value(124, UInt32.self)
    }
}
