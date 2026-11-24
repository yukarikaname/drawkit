// The C surface the platform shells call. Keep this file a mirror of `crates/core/src/ffi.rs`:
// when the two drift, the linker finds out on one platform and not the other.

import Foundation

@_silgen_name("dk_editor_new")
func dk_editor_new(_ width: UInt32, _ height: UInt32) -> UnsafeMutableRawPointer?
@_silgen_name("dk_open_file")
func dk_open_file(_ path: UnsafePointer<CChar>?) -> UnsafeMutableRawPointer?
@_silgen_name("dk_editor_clone")
func dk_editor_clone(_ handle: UnsafeMutableRawPointer?) -> UnsafeMutableRawPointer?
@_silgen_name("dk_editor_free")
func dk_editor_free(_ handle: UnsafeMutableRawPointer?)

@_silgen_name("dk_color_space")
func dk_color_space(_ handle: UnsafeMutableRawPointer?) -> Int32
@_silgen_name("dk_set_color_space")
func dk_set_color_space(_ handle: UnsafeMutableRawPointer?, _ space: Int32)
@_silgen_name("dk_dynamic_range")
func dk_dynamic_range(_ handle: UnsafeMutableRawPointer?) -> Int32
@_silgen_name("dk_set_dynamic_range")
func dk_set_dynamic_range(_ handle: UnsafeMutableRawPointer?, _ range: Int32)
@_silgen_name("dk_text")
func dk_text(_ key: UnsafePointer<CChar>?, _ language: UnsafePointer<CChar>?,
             _ out: UnsafeMutablePointer<CChar>?, _ capacity: Int) -> Int32
@_silgen_name("dk_width")
func dk_width(_ handle: UnsafeMutableRawPointer?) -> UInt32
@_silgen_name("dk_height")
func dk_height(_ handle: UnsafeMutableRawPointer?) -> UInt32

@_silgen_name("dk_set_brush")
func dk_set_brush(_ handle: UnsafeMutableRawPointer?, _ kind: Int32)
@_silgen_name("dk_brush")
func dk_brush(_ handle: UnsafeMutableRawPointer?) -> Int32
@_silgen_name("dk_set_mirror")
func dk_set_mirror(_ handle: UnsafeMutableRawPointer?, _ mode: Int32)
@_silgen_name("dk_set_mirror_angle")
func dk_set_mirror_angle(_ handle: UnsafeMutableRawPointer?, _ degrees: Float, _ both: Int32)
@_silgen_name("dk_mirror_angle")
func dk_mirror_angle(_ handle: UnsafeMutableRawPointer?) -> Float
@_silgen_name("dk_mirror_both")
func dk_mirror_both(_ handle: UnsafeMutableRawPointer?) -> Int32
@_silgen_name("dk_set_stabilization")
func dk_set_stabilization(_ handle: UnsafeMutableRawPointer?, _ strength: Float)
@_silgen_name("dk_stroke_begin")
func dk_stroke_begin(_ handle: UnsafeMutableRawPointer?, _ time_ms: UInt64)
@_silgen_name("dk_stroke_end")
func dk_stroke_end(_ handle: UnsafeMutableRawPointer?)
@_silgen_name("dk_stroke")
func dk_stroke(
    _ handle: UnsafeMutableRawPointer?, _ x0: Float, _ y0: Float, _ x1: Float, _ y1: Float,
    _ radius: Float, _ pressure: Float,
    _ red: Float, _ green: Float, _ blue: Float, _ alpha: Float, _ time_ms: UInt64)

@_silgen_name("dk_stroke_ex")
func dk_stroke_ex(
    _ handle: UnsafeMutableRawPointer?, _ x0: Float, _ y0: Float, _ x1: Float, _ y1: Float,
    _ radius: Float, _ pressure: Float, _ tiltX: Float, _ tiltY: Float,
    _ azimuth: Float, _ roll: Float,
    _ red: Float, _ green: Float, _ blue: Float, _ alpha: Float, _ time_ms: UInt64)
@_silgen_name("dk_erase_ex")
func dk_erase_ex(
    _ handle: UnsafeMutableRawPointer?, _ x0: Float, _ y0: Float, _ x1: Float, _ y1: Float,
    _ radius: Float, _ pressure: Float, _ tiltX: Float, _ tiltY: Float,
    _ azimuth: Float, _ roll: Float, _ time_ms: UInt64)
@_silgen_name("dk_erase")
func dk_erase(_ handle: UnsafeMutableRawPointer?, _ x0: Float, _ y0: Float, _ x1: Float,
              _ y1: Float, _ radius: Float, _ pressure: Float, _ time_ms: UInt64)
@_silgen_name("dk_fill")
func dk_fill(
    _ handle: UnsafeMutableRawPointer?, _ x: UInt32, _ y: UInt32, _ tolerance: Float,
    _ red: Float, _ green: Float, _ blue: Float, _ alpha: Float, _ time_ms: UInt64)
@_silgen_name("dk_remove_paper")
func dk_remove_paper(_ handle: UnsafeMutableRawPointer?) -> Int32
@_silgen_name("dk_set_prediction")
func dk_set_prediction(_ handle: UnsafeMutableRawPointer?, _ enabled: Int32)
@_silgen_name("dk_set_stroke_dedupe")
func dk_set_stroke_dedupe(_ handle: UnsafeMutableRawPointer?, _ enabled: Int32)
@_silgen_name("dk_set_fill_snap")
func dk_set_fill_snap(_ handle: UnsafeMutableRawPointer?, _ enabled: Int32)
@_silgen_name("dk_fill_ex")
func dk_fill_ex(
    _ handle: UnsafeMutableRawPointer?, _ x: UInt32, _ y: UInt32, _ tolerance: Float,
    _ close: UInt32, _ expand: UInt32, _ feather: UInt32,
    _ red: Float, _ green: Float, _ blue: Float, _ alpha: Float, _ time_ms: UInt64)

@_silgen_name("dk_can_undo")
func dk_can_undo(_ handle: UnsafeMutableRawPointer?) -> Int32
@_silgen_name("dk_can_redo")
func dk_can_redo(_ handle: UnsafeMutableRawPointer?) -> Int32
@_silgen_name("dk_undo")
func dk_undo(_ handle: UnsafeMutableRawPointer?) -> Int32
@_silgen_name("dk_redo")
func dk_redo(_ handle: UnsafeMutableRawPointer?) -> Int32
@_silgen_name("dk_smudge_ex")
func dk_smudge_ex(
    _ handle: UnsafeMutableRawPointer?, _ x0: Float, _ y0: Float, _ x1: Float, _ y1: Float,
    _ radius: Float, _ pressure: Float, _ strength: Float, _ time: UInt64)
@_silgen_name("dk_clear")
func dk_clear(
    _ handle: UnsafeMutableRawPointer?, _ red: Float, _ green: Float, _ blue: Float, _ alpha: Float)

@_silgen_name("dk_composite_linear")
func dk_composite_linear(_ handle: UnsafeMutableRawPointer?,
                         _ out: UnsafeMutablePointer<Float>?) -> Int32
@_silgen_name("dk_composite_linear_rect")
func dk_composite_linear_rect(_ handle: UnsafeMutableRawPointer?, _ x: UInt32, _ y: UInt32,
                              _ width: UInt32, _ height: UInt32,
                              _ out: UnsafeMutablePointer<Float>?) -> Int32
@_silgen_name("dk_composite")
func dk_composite(_ handle: UnsafeMutableRawPointer?, _ out: UnsafeMutablePointer<UInt8>?) -> Int32
@_silgen_name("dk_load_project")
func dk_load_project(_ handle: UnsafeMutableRawPointer?, _ path: UnsafePointer<CChar>?) -> Int32
@_silgen_name("dk_take_dirty")
func dk_take_dirty(_ handle: UnsafeMutableRawPointer?, _ out: UnsafeMutablePointer<UInt32>?) -> Int32
@_silgen_name("dk_composite_rect")
func dk_composite_rect(
    _ handle: UnsafeMutableRawPointer?, _ x: UInt32, _ y: UInt32, _ width: UInt32, _ height: UInt32,
    _ out: UnsafeMutablePointer<UInt8>?) -> Int32

@_silgen_name("dk_pick")
func dk_pick(_ handle: UnsafeMutableRawPointer?, _ x: UInt32, _ y: UInt32,
             _ out: UnsafeMutablePointer<Float>?) -> Int32

@_silgen_name("dk_record_enabled")
func dk_record_enabled(_ handle: UnsafeMutableRawPointer?, _ enabled: Int32)
@_silgen_name("dk_recording_ms")
func dk_recording_ms(_ handle: UnsafeMutableRawPointer?) -> UInt64
@_silgen_name("dk_overview")
func dk_overview(
    _ handle: UnsafeMutableRawPointer?, _ width: UInt32, _ height: UInt32,
    _ out: UnsafeMutablePointer<UInt8>?) -> Int32
@_silgen_name("dk_video_frame")
func dk_video_frame(
    _ handle: UnsafeMutableRawPointer?, _ timeMs: UInt64,
    _ width: UInt32, _ height: UInt32, _ out: UnsafeMutablePointer<UInt8>?) -> Int32
@_silgen_name("dk_recording_events")
func dk_recording_events(_ handle: UnsafeMutableRawPointer?) -> UInt32
@_silgen_name("dk_replay_composite")
func dk_replay_composite(
    _ handle: UnsafeMutableRawPointer?, _ time_ms: UInt64, _ out: UnsafeMutablePointer<UInt8>?) -> Int32

@_silgen_name("dk_import_kra")
func dk_import_kra(_ handle: UnsafeMutableRawPointer?, _ path: UnsafePointer<CChar>?) -> Int32
@_silgen_name("dk_import_psd")
func dk_import_psd(_ handle: UnsafeMutableRawPointer?, _ path: UnsafePointer<CChar>?) -> Int32
@_silgen_name("dk_export_kra")
func dk_export_kra(_ handle: UnsafeMutableRawPointer?, _ path: UnsafePointer<CChar>?) -> Int32
@_silgen_name("dk_export_psd")
func dk_export_psd(_ handle: UnsafeMutableRawPointer?, _ path: UnsafePointer<CChar>?) -> Int32
@_silgen_name("dk_export_layers")
func dk_export_layers(_ handle: UnsafeMutableRawPointer?, _ directory: UnsafePointer<CChar>?) -> UInt32
@_silgen_name("dk_export_jpeg")
func dk_export_jpeg(
    _ handle: UnsafeMutableRawPointer?, _ path: UnsafePointer<CChar>?, _ quality: Int32) -> Int32
@_silgen_name("dk_export_webp")
func dk_export_webp(_ handle: UnsafeMutableRawPointer?, _ path: UnsafePointer<CChar>?) -> Int32
@_silgen_name("dk_export_exr")
func dk_export_exr(_ handle: UnsafeMutableRawPointer?, _ path: UnsafePointer<CChar>?) -> Int32
@_silgen_name("dk_save_tape")
func dk_save_tape(_ handle: UnsafeMutableRawPointer?, _ path: UnsafePointer<CChar>?) -> Int32
@_silgen_name("dk_layer_thumbnail")
func dk_layer_thumbnail(_ handle: UnsafeMutableRawPointer?, _ index: UInt32, _ width: UInt32,
                        _ height: UInt32, _ out: UnsafeMutablePointer<UInt8>?) -> Int32
@_silgen_name("dk_layer_add_image")
func dk_layer_add_image(_ handle: UnsafeMutableRawPointer?, _ width: UInt32, _ height: UInt32,
                        _ pixels: UnsafePointer<UInt8>?) -> Int32
@_silgen_name("dk_layer_set_name")
func dk_layer_set_name(_ handle: UnsafeMutableRawPointer?, _ index: UInt32,
                       _ name: UnsafePointer<CChar>) -> Int32
@_silgen_name("dk_layer_name")
func dk_layer_name(_ handle: UnsafeMutableRawPointer?, _ index: UInt32,
                   _ out: UnsafeMutablePointer<UInt8>?, _ capacity: UInt32) -> Int32
@_silgen_name("dk_layer_count")
func dk_layer_count(_ handle: UnsafeMutableRawPointer?) -> UInt32
@_silgen_name("dk_layer_active")
func dk_layer_active(_ handle: UnsafeMutableRawPointer?) -> UInt32
@_silgen_name("dk_layer_select")
func dk_layer_select(_ handle: UnsafeMutableRawPointer?, _ index: UInt32)
@_silgen_name("dk_layer_add")
func dk_layer_add(_ handle: UnsafeMutableRawPointer?) -> UInt32
@_silgen_name("dk_layer_kind")
func dk_layer_kind(_ handle: UnsafeMutableRawPointer?, _ index: UInt32) -> Int32
@_silgen_name("dk_layer_parent")
func dk_layer_parent(_ handle: UnsafeMutableRawPointer?, _ index: UInt32) -> Int32
@_silgen_name("dk_layer_add_fill")
func dk_layer_add_fill(_ handle: UnsafeMutableRawPointer?, _ red: Float, _ green: Float,
                       _ blue: Float, _ alpha: Float) -> UInt32
@_silgen_name("dk_layer_add_group")
func dk_layer_add_group(_ handle: UnsafeMutableRawPointer?) -> UInt32
@_silgen_name("dk_layer_clipping")
func dk_layer_clipping(_ handle: UnsafeMutableRawPointer?, _ index: UInt32) -> Int32
@_silgen_name("dk_layer_set_clipping")
func dk_layer_set_clipping(_ handle: UnsafeMutableRawPointer?, _ index: UInt32, _ clipping: Int32)
@_silgen_name("dk_layer_alpha_locked")
func dk_layer_alpha_locked(_ handle: UnsafeMutableRawPointer?, _ index: UInt32) -> Int32
@_silgen_name("dk_layer_set_alpha_locked")
func dk_layer_set_alpha_locked(
    _ handle: UnsafeMutableRawPointer?, _ index: UInt32, _ locked: Int32)
@_silgen_name("dk_layer_locked")
func dk_layer_locked(_ handle: UnsafeMutableRawPointer?, _ index: UInt32) -> Int32
@_silgen_name("dk_layer_set_locked")
func dk_layer_set_locked(_ handle: UnsafeMutableRawPointer?, _ index: UInt32, _ locked: Int32)
@_silgen_name("dk_take_gpu_dirty")
func dk_take_gpu_dirty(
    _ handle: UnsafeMutableRawPointer?, _ index: UInt32,
    _ out: UnsafeMutablePointer<UInt32>?) -> Int32
@_silgen_name("dk_layer_rect")
func dk_layer_rect(
    _ handle: UnsafeMutableRawPointer?, _ index: UInt32,
    _ x: UInt32, _ y: UInt32, _ width: UInt32, _ height: UInt32,
    _ out: UnsafeMutablePointer<UInt8>?) -> Int32
@_silgen_name("dk_layer_effective_visible")
func dk_layer_effective_visible(_ handle: UnsafeMutableRawPointer?, _ index: UInt32) -> Int32
@_silgen_name("dk_layer_effective_opacity")
func dk_layer_effective_opacity(_ handle: UnsafeMutableRawPointer?, _ index: UInt32) -> Float
@_silgen_name("dk_layer_fill")
func dk_layer_fill(
    _ handle: UnsafeMutableRawPointer?, _ index: UInt32, _ out: UnsafeMutablePointer<Float>?) -> Int32
@_silgen_name("dk_layer_blend")
func dk_layer_blend(_ handle: UnsafeMutableRawPointer?, _ index: UInt32) -> Int32
@_silgen_name("dk_layer_set_blend")
func dk_layer_set_blend(_ handle: UnsafeMutableRawPointer?, _ index: UInt32, _ blend: Int32)
@_silgen_name("dk_layer_expanded")
func dk_layer_expanded(_ handle: UnsafeMutableRawPointer?, _ index: UInt32) -> Int32
@_silgen_name("dk_layer_set_expanded")
func dk_layer_set_expanded(_ handle: UnsafeMutableRawPointer?, _ index: UInt32, _ expanded: Int32)
@_silgen_name("dk_layer_move")
func dk_layer_move(_ handle: UnsafeMutableRawPointer?, _ index: UInt32, _ steps: Int32) -> Int32
@_silgen_name("dk_layer_remove")
func dk_layer_remove(_ handle: UnsafeMutableRawPointer?) -> Int32
@_silgen_name("dk_layer_visible")
func dk_layer_visible(_ handle: UnsafeMutableRawPointer?, _ index: UInt32) -> Int32
@_silgen_name("dk_layer_set_visible")
func dk_layer_set_visible(_ handle: UnsafeMutableRawPointer?, _ index: UInt32, _ visible: Int32)
@_silgen_name("dk_layer_opacity")
func dk_layer_opacity(_ handle: UnsafeMutableRawPointer?, _ index: UInt32) -> Float
@_silgen_name("dk_layer_set_opacity")
func dk_layer_set_opacity(_ handle: UnsafeMutableRawPointer?, _ index: UInt32, _ opacity: Float)

// The drawing mannequin, rendered by the GPU library. Its lifetime is separate from the painting
// document: a pose is not a canvas undo step, and the shell may drop the figure without touching
// the image. A null handle means the machine has no usable GPU adapter; there is no CPU fallback.

@_silgen_name("dk_figure_available")
func dk_figure_available() -> Int32
@_silgen_name("dk_figure_new")
func dk_figure_new(_ preset: UInt32) -> UnsafeMutableRawPointer?
@_silgen_name("dk_figure_free")
func dk_figure_free(_ handle: UnsafeMutableRawPointer?)
@_silgen_name("dk_figure_height")
func dk_figure_height(_ handle: UnsafeMutableRawPointer?) -> Float
@_silgen_name("dk_figure_set_preset")
func dk_figure_set_preset(_ handle: UnsafeMutableRawPointer?, _ preset: UInt32)
@_silgen_name("dk_figure_pose")
func dk_figure_pose(_ handle: UnsafeMutableRawPointer?, _ preset: UInt32)
@_silgen_name("dk_figure_reset")
func dk_figure_reset(_ handle: UnsafeMutableRawPointer?)
@_silgen_name("dk_figure_mirror")
func dk_figure_mirror(_ handle: UnsafeMutableRawPointer?)
@_silgen_name("dk_figure_rotate")
func dk_figure_rotate(_ handle: UnsafeMutableRawPointer?, _ joint: UInt32,
                      _ axisX: Float, _ axisY: Float, _ axisZ: Float, _ radians: Float) -> Int32
@_silgen_name("dk_figure_aim")
func dk_figure_aim(_ handle: UnsafeMutableRawPointer?, _ joint: UInt32,
                   _ targetX: Float, _ targetY: Float, _ targetZ: Float) -> Int32
@_silgen_name("dk_figure_ik")
func dk_figure_ik(_ handle: UnsafeMutableRawPointer?, _ upper: UInt32, _ lower: UInt32,
                  _ end: UInt32, _ targetX: Float, _ targetY: Float, _ targetZ: Float,
                  _ poleX: Float, _ poleY: Float, _ poleZ: Float) -> Int32
@_silgen_name("dk_figure_pick")
func dk_figure_pick(_ handle: UnsafeMutableRawPointer?, _ originX: Float, _ originY: Float,
                    _ originZ: Float, _ directionX: Float, _ directionY: Float,
                    _ directionZ: Float, _ radius: Float) -> Int32
@_silgen_name("dk_figure_joint")
func dk_figure_joint(_ handle: UnsafeMutableRawPointer?, _ joint: UInt32,
                     _ out: UnsafeMutablePointer<Float>?) -> Int32
@_silgen_name("dk_figure_render")
func dk_figure_render(_ handle: UnsafeMutableRawPointer?, _ width: UInt32, _ height: UInt32,
                      _ yaw: Float, _ pitch: Float, _ distance: Float,
                      _ out: UnsafeMutablePointer<UInt8>?) -> Int32

// A wgpu swapchain on a CAMetalLayer, for the GPU-presented canvas.
@_silgen_name("dk_canvas_surface_new")
func dk_canvas_surface_new(_ layer: UnsafeMutableRawPointer?, _ width: UInt32,
                           _ height: UInt32, _ hdr: Int32,
                           _ colorSpace: Int32) -> UnsafeMutableRawPointer?
@_silgen_name("dk_canvas_surface_set_exposure")
func dk_canvas_surface_set_exposure(_ handle: UnsafeMutableRawPointer?, _ exposure: Float)
@_silgen_name("dk_canvas_surface_hdr")
func dk_canvas_surface_hdr(_ handle: UnsafeMutableRawPointer?) -> Int32
@_silgen_name("dk_canvas_surface_present_hdr")
func dk_canvas_surface_present_hdr(_ handle: UnsafeMutableRawPointer?,
                                   _ pixels: UnsafePointer<Float>?,
                                   _ canvasWidth: UInt32, _ canvasHeight: UInt32,
                                   _ paperX: Float, _ paperY: Float,
                                   _ paperWidth: Float, _ paperHeight: Float,
                                   _ dirtyX: UInt32, _ dirtyY: UInt32,
                                   _ dirtyWidth: UInt32, _ dirtyHeight: UInt32) -> Int32
@_silgen_name("dk_canvas_surface_free")
func dk_canvas_surface_free(_ handle: UnsafeMutableRawPointer?)
@_silgen_name("dk_canvas_surface_resize")
func dk_canvas_surface_resize(_ handle: UnsafeMutableRawPointer?, _ width: UInt32, _ height: UInt32)
@_silgen_name("dk_canvas_surface_present")
func dk_canvas_surface_present(_ handle: UnsafeMutableRawPointer?, _ pixels: UnsafePointer<UInt8>?,
                               _ canvasWidth: UInt32, _ canvasHeight: UInt32,
                               _ paperX: Float, _ paperY: Float,
                               _ paperWidth: Float, _ paperHeight: Float,
                               _ dirtyX: UInt32, _ dirtyY: UInt32,
                               _ dirtyWidth: UInt32, _ dirtyHeight: UInt32) -> Int32

// Shared picker images: a hue wheel and one saturation/value square per hue.
@_silgen_name("dk_is_clear")
func dk_is_clear(_ handle: UnsafeMutableRawPointer?) -> Int32
@_silgen_name("dk_read_palette")
func dk_read_palette(_ path: UnsafePointer<CChar>, _ out: UnsafeMutablePointer<Float>,
                     _ maxColors: UInt32) -> Int32
@_silgen_name("dk_write_palette")
func dk_write_palette(_ path: UnsafePointer<CChar>, _ colors: UnsafePointer<Float>,
                      _ count: UInt32, _ format: Int32) -> Int32
@_silgen_name("dk_set_auto_shape")
func dk_set_auto_shape(_ handle: UnsafeMutableRawPointer?, _ enabled: Int32)
@_silgen_name("dk_auto_shape")
func dk_auto_shape(_ handle: UnsafeMutableRawPointer?) -> Int32
@_silgen_name("dk_palette")
func dk_palette(_ handle: UnsafeMutableRawPointer?, _ max: UInt32,
                _ out: UnsafeMutablePointer<Float>?) -> Int32
@_silgen_name("dk_picker_wheel")
func dk_picker_wheel(_ size: UInt32, _ out: UnsafeMutablePointer<UInt8>?) -> Int32
@_silgen_name("dk_picker_sv")
func dk_picker_sv(_ hue: Float, _ width: UInt32, _ height: UInt32,
                  _ out: UnsafeMutablePointer<UInt8>?) -> Int32
