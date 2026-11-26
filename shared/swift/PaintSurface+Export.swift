// Reading the composed canvas back for a save or a Copy Merged.
//
// The resident stack keeps the finished frame on the GPU. An export resamples that frame into the
// requested size on the same device and reads the one texture back, so a save no longer has to
// materialise every layer into the core first.

import Foundation

@_silgen_name("dk_canvas_surface_export_linear")
private func dk_canvas_surface_export_linear(
    _ handle: UnsafeMutableRawPointer?, _ width: UInt32, _ height: UInt32,
    _ out: UnsafeMutablePointer<Float>?, _ capacity: UInt32) -> Int32

@_silgen_name("dk_canvas_surface_export_gpu")
private func dk_canvas_surface_export_gpu(
    _ handle: UnsafeMutableRawPointer?, _ width: UInt32, _ height: UInt32,
    _ out: UnsafeMutablePointer<UInt8>?, _ capacity: UInt32) -> Int32

@_silgen_name("dk_canvas_surface_layer_alpha_at")
private func dk_canvas_surface_layer_alpha_at(
    _ handle: UnsafeMutableRawPointer?, _ index: UInt32, _ x: UInt32, _ y: UInt32) -> Float

@_silgen_name("dk_canvas_surface_set_export_reference")
private func dk_canvas_surface_set_export_reference(
    _ handle: UnsafeMutableRawPointer?, _ index: UInt32, _ reference: Int32)

extension PaintSurface {
    /// Premultiplied linear Display P3, for an EXR. Nil when the stack is eight-bit or refuses.
    func exportLinear(width: Int, height: Int) -> [Float]? {
        guard let handle, width > 0, height > 0 else { return nil }
        let count = width * height * 4
        var pixels = [Float](repeating: 0, count: count)
        let exported = pixels.withUnsafeMutableBufferPointer {
            dk_canvas_surface_export_linear(
                handle, UInt32(width), UInt32(height), $0.baseAddress, UInt32(count))
        }
        return exported != 0 ? pixels : nil
    }

    /// The composed layers at `width` by `height` as premultiplied RGBA8. Nil when the surface
    /// cannot export (an extended-range stack, no device) and the caller must materialise.
    func exportPixels(width: Int, height: Int) -> [UInt8]? {
        guard let handle, width > 0, height > 0 else { return nil }
        var pixels = [UInt8](repeating: 0, count: width * height * 4)
        let exported = pixels.withUnsafeMutableBufferPointer {
            guard let base = $0.baseAddress else { return Int32(0) }
            return dk_canvas_surface_export_gpu(
                handle, UInt32(width), UInt32(height), base, UInt32($0.count))
        }
        return exported != 0 ? pixels : nil
    }

    /// The alpha of one layer pixel, read back from its resident tile. A live fill has to know
    /// whether the layer it writes is empty at the tap, and the CPU cache does not hold it.
    func layerAlpha(index: Int, x: Int, y: Int) -> Float {
        guard let handle, x >= 0, y >= 0 else { return -1 }
        return dk_canvas_surface_layer_alpha_at(handle, UInt32(index), UInt32(x), UInt32(y))
    }

    /// Whether layer `index` is left out of an export. The canvas still shows it.
    func setExportReference(index: Int, reference: Bool) {
        guard let handle, index >= 0 else { return }
        dk_canvas_surface_set_export_reference(handle, UInt32(index), reference ? 1 : 0)
    }
}
