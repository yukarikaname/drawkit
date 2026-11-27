// The resident surface's C entry points, split from the editor so each file stays small.

import Foundation

@_silgen_name("dk_canvas_surface_new")
func dk_canvas_surface_new(
    _ layer: UnsafeMutableRawPointer?, _ width: UInt32,
    _ height: UInt32, _ hdr: Int32,
    _ colorSpace: Int32
) -> UnsafeMutableRawPointer?
@_silgen_name("dk_canvas_surface_set_exposure")
func dk_canvas_surface_set_exposure(_ handle: UnsafeMutableRawPointer?, _ exposure: Float)
@_silgen_name("dk_canvas_surface_hdr")
func dk_canvas_surface_hdr(_ handle: UnsafeMutableRawPointer?) -> Int32
@_silgen_name("dk_canvas_surface_present_hdr")
func dk_canvas_surface_present_hdr(
    _ handle: UnsafeMutableRawPointer?,
    _ pixels: UnsafePointer<Float>?,
    _ canvasWidth: UInt32, _ canvasHeight: UInt32,
    _ paperX: Float, _ paperY: Float,
    _ paperWidth: Float, _ paperHeight: Float,
    _ dirtyX: UInt32, _ dirtyY: UInt32,
    _ dirtyWidth: UInt32, _ dirtyHeight: UInt32
) -> Int32
@_silgen_name("dk_canvas_surface_free")
func dk_canvas_surface_free(_ handle: UnsafeMutableRawPointer?)
@_silgen_name("dk_canvas_surface_resize")
func dk_canvas_surface_resize(_ handle: UnsafeMutableRawPointer?, _ width: UInt32, _ height: UInt32)
@_silgen_name("dk_canvas_surface_present")
func dk_canvas_surface_present(
    _ handle: UnsafeMutableRawPointer?, _ pixels: UnsafePointer<UInt8>?,
    _ canvasWidth: UInt32, _ canvasHeight: UInt32,
    _ paperX: Float, _ paperY: Float,
    _ paperWidth: Float, _ paperHeight: Float,
    _ dirtyX: UInt32, _ dirtyY: UInt32,
    _ dirtyWidth: UInt32, _ dirtyHeight: UInt32
) -> Int32

// Shared picker images: a hue wheel and one saturation/value square per hue.
@_silgen_name("dk_is_clear")
func dk_is_clear(_ handle: UnsafeMutableRawPointer?) -> Int32
@_silgen_name("dk_read_palette")
func dk_read_palette(
    _ path: UnsafePointer<CChar>, _ out: UnsafeMutablePointer<Float>,
    _ maxColors: UInt32
) -> Int32
@_silgen_name("dk_write_palette")
func dk_write_palette(
    _ path: UnsafePointer<CChar>, _ colors: UnsafePointer<Float>,
    _ count: UInt32, _ format: Int32
) -> Int32
@_silgen_name("dk_set_quick_shape")
func dk_set_quick_shape(_ handle: UnsafeMutableRawPointer?, _ enabled: Int32)
@_silgen_name("dk_set_auto_shape")
func dk_set_auto_shape(_ handle: UnsafeMutableRawPointer?, _ enabled: Int32)
@_silgen_name("dk_auto_shape")
func dk_auto_shape(_ handle: UnsafeMutableRawPointer?) -> Int32
@_silgen_name("dk_auto_shape_snapped")
func dk_auto_shape_snapped(_ handle: UnsafeMutableRawPointer?) -> Int32
@_silgen_name("dk_palette")
func dk_palette(
    _ handle: UnsafeMutableRawPointer?, _ max: UInt32,
    _ out: UnsafeMutablePointer<Float>?
) -> Int32
@_silgen_name("dk_picker_wheel")
func dk_picker_wheel(_ size: UInt32, _ out: UnsafeMutablePointer<UInt8>?) -> Int32
@_silgen_name("dk_picker_sv")
func dk_picker_sv(
    _ hue: Float, _ width: UInt32, _ height: UInt32,
    _ out: UnsafeMutablePointer<UInt8>?
) -> Int32
