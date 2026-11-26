// The live fill on the resident-layer canvas: the core builds the mask, the surface writes it.
//
// macOS has no CPU fill path. The core's `dk_fill_begin`/`step`/`finish` stay for shells and tests
// that have no resident stack; calling them here would paint the CPU cache while the resident
// tiles kept the old pixels. A fill without a usable surface is reported, not painted.

import SwiftUI
import AppKit

// The resident-layer fill: the core builds the mask, the surface writes it into the tiles.
@_silgen_name("dk_fill_gpu_begin")
private func dk_fill_gpu_begin(
    _ handle: UnsafeMutableRawPointer?, _ x: UInt32, _ y: UInt32, _ tolerance: Float,
    _ close: UInt32, _ expand: UInt32, _ feather: UInt32,
    _ red: Float, _ green: Float, _ blue: Float, _ alpha: Float, _ timeMs: UInt64,
    _ mask: UnsafeMutablePointer<Float>?, _ maskLength: UInt32,
    _ rect: UnsafeMutablePointer<UInt32>?, _ colour: UnsafeMutablePointer<Float>?) -> Int32
@_silgen_name("dk_fill_gpu_finish")
private func dk_fill_gpu_finish(
    _ handle: UnsafeMutableRawPointer?, _ rects: UnsafePointer<UInt32>?, _ count: UInt32) -> Int32
@_silgen_name("dk_fill_gpu_cancel")
private func dk_fill_gpu_cancel(_ handle: UnsafeMutableRawPointer?) -> Int32
@_silgen_name("dk_canvas_surface_apply_fill")
private func dk_canvas_surface_apply_fill(
    _ handle: UnsafeMutableRawPointer?, _ layer: UInt32, _ mask: UnsafePointer<Float>?,
    _ width: UInt32, _ height: UInt32, _ red: Float, _ green: Float, _ blue: Float,
    _ alpha: Float, _ alphaLocked: Int32, _ under: Int32,
    _ dirty: UnsafeMutablePointer<UInt32>?) -> Int32

extension PaintModel {
    /// Start a fill: the core prepares the mask and the surface writes it into the resident tiles
    /// in one pass. The whole fill is one undo step and one tape event.
    @discardableResult
    func beginFill(at point: CGPoint, tolerance: Float, close: Int, expand: Int, feather: Int,
                   ink: Ink) -> Bool {
        fill(at: point, tolerance: tolerance, close: close, expand: expand,
             feather: feather, ink: ink)
    }

    /// The shell has no resident surface to fill into. macOS does not keep a CPU fill path, so the
    /// only honest answer is to say so rather than paint somewhere the canvas will not show.
    private func reportFillUnavailable() {
        NSSound.beep()
        flash(String(localized: "Fill needs the GPU canvas"), error: true)
    }

    /// Wait for a spreading fill to finish. The resident fill is written in one pass, so this is
    /// only an entry point the stroke, undo and frame code can call unconditionally.
    func finishFillAnimation() {
        fillTask?.cancel()
        fillTask = nil
    }

    /// Abandon a spreading fill. There is nothing to reveal and no pixels to put back.
    func cancelFillAnimation() {
        fillTask?.cancel()
        fillTask = nil
    }

    /// Apply a fill into the resident layer tiles, with the tile journal as its undo record.
    ///
    /// The core still decides the coverage; only the write moved. False means there is no usable
    /// surface or the surface refused the write; nothing was painted.
    @discardableResult
    func fill(at point: CGPoint, tolerance: Float,
              close: Int, expand: Int, feather: Int, ink: Ink) -> Bool {
        finishFillAnimation()
        guard gpuLiveStroke, let surface = paintSurface, surface.supportsLayers else {
            reportFillUnavailable()
            return false
        }
        // The mask is decided from the composed document, so the CPU cache has to catch up with
        // any resident pixels a previous stroke left there.
        materializeCanvas()
        var mask = [Float](repeating: 0, count: width * height)
        var rect = [UInt32](repeating: 0, count: 7)
        var colour = [Float](repeating: 0, count: 4)
        let prepared = mask.withUnsafeMutableBufferPointer { maskBuffer in
            rect.withUnsafeMutableBufferPointer { rectBuffer in
                colour.withUnsafeMutableBufferPointer { colourBuffer in
                    dk_fill_gpu_begin(handle, UInt32(point.x), UInt32(point.y), tolerance,
                                      UInt32(close), UInt32(expand), UInt32(feather),
                                      ink.red, ink.green, ink.blue, ink.alpha, now,
                                      maskBuffer.baseAddress, UInt32(maskBuffer.count),
                                      rectBuffer.baseAddress, colourBuffer.baseAddress)
                }
            }
        }
        guard prepared != 0 else { return false }
        // An extended-range stack holds linear Display P3, so the ink goes through the same
        // conversion a GPU stroke uses.
        var rgb = (colour[0], colour[1], colour[2])
        if surface.extended {
            var linear = [Float](repeating: 0, count: 3)
            let converted = linear.withUnsafeMutableBufferPointer {
                dk_linear_colour(handle, colour[0], colour[1], colour[2], $0.baseAddress)
            }
            if converted != 0 {
                rgb = (linear[0], linear[1], linear[2])
            }
        }
        var dirty = [UInt32](repeating: 0, count: 4)
        let applied = mask.withUnsafeBufferPointer { maskBuffer in
            dirty.withUnsafeMutableBufferPointer { dirtyBuffer in
                dk_canvas_surface_apply_fill(
                    surface.handle, rect[0], maskBuffer.baseAddress,
                    UInt32(width), UInt32(height), rgb.0, rgb.1, rgb.2, colour[3],
                    rect[6] != 0 ? 1 : 0, rect[5] != 0 ? 1 : 0,
                    dirtyBuffer.baseAddress) != 0
            }
        }
        guard applied else {
            // The mask was built but no tile changed, or the surface refused it. Either way no
            // pixel moved, so there is nothing to fall back to.
            _ = dk_fill_gpu_cancel(handle)
            reportFillUnavailable()
            return false
        }
        _ = dirty.withUnsafeBufferPointer {
            dk_fill_gpu_finish(handle, $0.baseAddress, 1)
        }
        isBlank = false
        gpuStale = true
        edited()
        revision &+= 1
        scheduleThumbnails()
        if frameCount > 1 {
            refreshActiveFrameThumbnail()
        }
        refresh()
        requestPresent?()
        return true
    }
}
