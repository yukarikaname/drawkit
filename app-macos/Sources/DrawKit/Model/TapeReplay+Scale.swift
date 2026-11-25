// Uploading the layers the tape does not reproduce. At 1:1 the document pixels are sent as they
// are; at another size they are resampled once, because an imported layer has no strokes to
// re-render.

import CoreGraphics
import Foundation

extension TapeReplay {
    /// A layer that is not painted by the tape: an import, a bake or a fill. Fill and group layers
    /// have no pixels; only paint layers reach here.
    func uploadBase(_ index: Int, _ handle: UnsafeMutableRawPointer) -> Bool {
        if spec.pixelsPerUnit == 1 {
            return uploadFullSize(index, handle)
        }
        return uploadScaled(index, handle)
    }

    private func uploadFullSize(_ index: Int, _ handle: UnsafeMutableRawPointer) -> Bool {
        if isHdr {
            var pixels = [Float](repeating: 0, count: docWidth * docHeight * 4)
            let copied = pixels.withUnsafeMutableBufferPointer {
                dk_layer_rect_linear(core, UInt32(index), 0, 0, UInt32(docWidth),
                                     UInt32(docHeight), $0.baseAddress) != 0
            }
            guard copied else { return false }
            return pixels.withUnsafeBufferPointer {
                dk_gpu_compositor_upload_layer_linear(
                    handle, UInt32(index), UInt32(docWidth), UInt32(docHeight),
                    $0.baseAddress) != 0
            }
        }
        var pixels = [UInt8](repeating: 0, count: docWidth * docHeight * 4)
        let copied = pixels.withUnsafeMutableBytes {
            dk_layer_rect(core, UInt32(index), 0, 0, UInt32(docWidth), UInt32(docHeight),
                          $0.bindMemory(to: UInt8.self).baseAddress) != 0
        }
        guard copied else { return false }
        return pixels.withUnsafeBufferPointer {
            dk_gpu_compositor_upload_layer(
                handle, UInt32(index), 0, 0, UInt32(docWidth), UInt32(docHeight),
                $0.baseAddress) != 0
        }
    }

    /// Resample one eight-bit layer to the output size. The core stores straight alpha, so the
    /// premultiplied result of drawing into a bitmap context is unpremultiplied before upload.
    private func uploadScaled(_ index: Int, _ handle: UnsafeMutableRawPointer) -> Bool {
        guard !isHdr, let scaled = resampleLayer(index) else { return false }
        return scaled.withUnsafeBufferPointer {
            dk_gpu_compositor_upload_layer(
                handle, UInt32(index), 0, 0, UInt32(spec.width), UInt32(spec.height),
                $0.baseAddress) != 0
        }
    }

    private func resampleLayer(_ index: Int) -> [UInt8]? {
        var source = [UInt8](repeating: 0, count: docWidth * docHeight * 4)
        let copied = source.withUnsafeMutableBytes {
            dk_layer_rect(core, UInt32(index), 0, 0, UInt32(docWidth), UInt32(docHeight),
                          $0.bindMemory(to: UInt8.self).baseAddress) != 0
        }
        guard copied else { return nil }
        let space = PaintColorSpace(rawValue: Int(dk_color_space(core))) ?? .sRGB
        let colorSpace = space.cgColorSpace
        guard let provider = CGDataProvider(data: Data(source) as CFData),
              let image = CGImage(
                width: docWidth, height: docHeight, bitsPerComponent: 8, bitsPerPixel: 32,
                bytesPerRow: docWidth * 4, space: colorSpace,
                bitmapInfo: CGBitmapInfo(rawValue: CGImageAlphaInfo.last.rawValue),
                provider: provider, decode: nil, shouldInterpolate: true,
                intent: .defaultIntent)
        else { return nil }
        var target = [UInt8](repeating: 0, count: spec.width * spec.height * 4)
        let drawn = target.withUnsafeMutableBytes { buffer -> Bool in
            guard let context = CGContext(
                data: buffer.baseAddress, width: spec.width, height: spec.height,
                bitsPerComponent: 8, bytesPerRow: spec.width * 4, space: colorSpace,
                bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)
            else { return false }
            context.interpolationQuality = .high
            context.draw(image, in: CGRect(x: 0, y: 0, width: spec.width, height: spec.height))
            return true
        }
        guard drawn else { return nil }
        Self.unpremultiply(&target)
        return target
    }

    private static func unpremultiply(_ pixels: inout [UInt8]) {
        for at in stride(from: 0, to: pixels.count, by: 4) {
            let alpha = Int(pixels[at + 3])
            guard alpha > 0, alpha < 255 else { continue }
            for channel in 0..<3 {
                let straight = Int(pixels[at + channel]) * 255 / alpha
                pixels[at + channel] = UInt8(min(straight, 255))
            }
        }
    }
}
