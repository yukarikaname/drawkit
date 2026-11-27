// Copy, cut and paste as pixels. PNG is what the iOS pasteboard carries best, and the same pixels
// paste into another app. A paste becomes a new layer and one undo step.

import UIKit

extension Studio {
    /// What a copy takes: the selection, or the whole canvas.
    var copyRegion: CGRect {
        selection ?? CGRect(x: 0, y: 0, width: width, height: height)
    }

    /// Put the active layer (or the composed frame) on the general pasteboard.
    @discardableResult
    func copyPixels(merged: Bool = false) -> Bool {
        materializeCanvas()
        let region = copyRegion.integral
        let w = max(0, Int(region.width))
        let h = max(0, Int(region.height))
        guard w > 0, h > 0 else { return false }

        var bytes = [UInt8](repeating: 0, count: w * h * 4)
        let read = bytes.withUnsafeMutableBytes { buffer -> Bool in
            let out = buffer.bindMemory(to: UInt8.self).baseAddress
            if merged {
                return dk_composite_rect(
                    handle, UInt32(region.minX), UInt32(region.minY),
                    UInt32(w), UInt32(h), out) != 0
            }
            return dk_layer_rect(
                handle, UInt32(activeLayer), UInt32(region.minX),
                UInt32(region.minY), UInt32(w), UInt32(h), out) != 0
        }
        guard read, let image = clipboardImage(bytes, width: w, height: h) else { return false }
        UIPasteboard.general.image = UIImage(cgImage: image)
        return true
    }

    /// One layer as PNG (premultiplied, no paper), for a drag out of the layer list.
    func layerPNGData(index: Int) -> Data? {
        guard index >= 0, index < layers.count else { return nil }
        materializeCanvas()
        var pixels = [UInt8](repeating: 0, count: width * height * 4)
        let read = pixels.withUnsafeMutableBytes {
            dk_layer_rect(
                handle, UInt32(index), 0, 0, UInt32(width), UInt32(height),
                $0.bindMemory(to: UInt8.self).baseAddress) != 0
        }
        guard read, let image = clipboardImage(pixels, width: width, height: height) else {
            return nil
        }
        return UIImage(cgImage: image).pngData()
    }

    /// Copy, then empty the selection (or the layer when nothing is selected).
    @discardableResult
    func cutPixels() -> Bool {
        guard copyPixels() else { return false }
        clearLayer()
        return true
    }

    /// Paste the pasteboard as a new layer. It lands at the selection's corner, or centred when
    /// there is no selection; nothing is scaled, so a 1:1 copy stays 1:1.
    @discardableResult
    func pastePixels() -> Bool {
        materializeCanvas()
        guard let image = UIPasteboard.general.image, let source = image.cgImage else {
            return false
        }
        let originX = selection.map { Int($0.minX) } ?? (width - source.width) / 2
        let originY = selection.map { Int($0.minY) } ?? (height - source.height) / 2
        guard let buffer = pasteBuffer(source, originX: originX, originY: originY) else {
            return false
        }
        let added = buffer.withUnsafeBytes { raw in
            dk_layer_add_image(
                handle, UInt32(width), UInt32(height),
                raw.bindMemory(to: UInt8.self).baseAddress)
        }
        guard added != 0 else { return false }
        // Name it so it can be found again in the layer list.
        let layer = Int(dk_layer_active(handle))
        String(localized: "Pasted").withCString {
            _ = dk_layer_set_name(handle, UInt32(layer), $0)
        }
        refreshAll()
        refreshLayers()
        finishChange()
        return true
    }

    /// A premultiplied CGImage in the working colour space, for the pasteboard.
    private func clipboardImage(_ pixels: [UInt8], width: Int, height: Int) -> CGImage? {
        guard let provider = CGDataProvider(data: Data(pixels) as CFData) else { return nil }
        return CGImage(
            width: width, height: height, bitsPerComponent: 8, bitsPerPixel: 32,
            bytesPerRow: width * 4, space: colorSpace.cgColorSpace,
            bitmapInfo: CGBitmapInfo(rawValue: CGImageAlphaInfo.premultipliedLast.rawValue),
            provider: provider, decode: nil, shouldInterpolate: false, intent: .defaultIntent)
    }

    /// Draw the pasted image into a canvas-sized premultiplied buffer. Core surfaces are top-down
    /// and a bitmap context is bottom-up, so the context is flipped once before drawing.
    private func pasteBuffer(_ source: CGImage, originX: Int, originY: Int) -> [UInt8]? {
        var buffer = [UInt8](repeating: 0, count: width * height * 4)
        let drawn = buffer.withUnsafeMutableBytes { raw -> Bool in
            guard
                let context = CGContext(
                    data: raw.baseAddress, width: width, height: height, bitsPerComponent: 8,
                    bytesPerRow: width * 4, space: colorSpace.cgColorSpace,
                    bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue
                )
            else { return false }
            context.translateBy(x: 0, y: CGFloat(height))
            context.scaleBy(x: 1, y: -1)
            context.interpolationQuality = .none
            context.draw(
                source,
                in: CGRect(
                    x: originX, y: originY,
                    width: source.width, height: source.height))
            return true
        }
        return drawn ? buffer : nil
    }
}
