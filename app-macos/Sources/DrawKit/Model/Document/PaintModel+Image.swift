// A CGImage over the live pixel buffer.

import CoreGraphics
import Foundation

extension PaintModel {
    /// A `CGImage` over the live buffer. The provider retains the buffer, so the image may outlive
    /// this model without dangling; the image must be recreated after the buffer is written.
    func makeImage() -> CGImage? {
        makeImage(buffer: pixels, width: width, height: height)
    }

    /// A preview of the document at its own size.
    func makeImage(buffer: PixelBuffer) -> CGImage? {
        makeImage(buffer: buffer, width: width, height: height)
    }

    func makeImage(buffer: PixelBuffer, width: Int, height: Int) -> CGImage? {
        let retained = Unmanaged.passRetained(buffer)
        guard
            let provider = CGDataProvider(
                dataInfo: retained.toOpaque(),
                data: buffer.pointer,
                size: buffer.count,
                releaseData: { info, _, _ in
                    guard let info else { return }
                    Unmanaged<PixelBuffer>.fromOpaque(info).release()
                })
        else {
            retained.release()
            return nil
        }
        return CGImage(
            width: width, height: height, bitsPerComponent: 8, bitsPerPixel: 32,
            bytesPerRow: width * 4, space: colorSpace.cgColorSpace,
            bitmapInfo: CGBitmapInfo(rawValue: CGImageAlphaInfo.premultipliedLast.rawValue),
            provider: provider, decode: nil, shouldInterpolate: false, intent: .defaultIntent)
    }
}
