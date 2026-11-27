// Entry points for picking and editing a recorded stroke.

import Foundation

@_silgen_name("dk_recolor_stroke")
func dk_recolor_stroke(
    _ handle: UnsafeMutableRawPointer?, _ group: UInt64,
    _ red: Float, _ green: Float, _ blue: Float, _ alpha: Float
) -> Int32
@_silgen_name("dk_delete_stroke")
func dk_delete_stroke(_ handle: UnsafeMutableRawPointer?, _ group: UInt64) -> Int32
@_silgen_name("dk_stroke_at")
func dk_stroke_at(_ handle: UnsafeMutableRawPointer?, _ x: Float, _ y: Float) -> UInt64
@_silgen_name("dk_stroke_bounds")
func dk_stroke_bounds(
    _ handle: UnsafeMutableRawPointer?, _ group: UInt64,
    _ out: UnsafeMutablePointer<UInt32>?
) -> Int32
