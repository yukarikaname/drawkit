// Copy, cut and paste as pixels. The pasteboard carries both TIFF and PNG, so a copy lands in
// Preview, a chat window or another canvas; a paste becomes a new layer and one undo step.
//
// The active layer is copied by default. "Copy Merged" takes the composed frame instead, which is
// what a person wants when the painting is spread over several layers.

import AppKit

extension PaintModel {
    /// What a copy takes: the selection, or the whole canvas.
    var copyRegion: SurfaceRect {
        selection ?? SurfaceRect(x: 0, y: 0, width: width, height: height)
    }

    /// Put the active layer (or the composed frame) on the general pasteboard.
    @discardableResult
    func copyPixels(merged: Bool = false) -> Bool {
        let region = copyRegion
        let count = region.width * region.height * 4
        guard region.width > 0, region.height > 0, count > 0 else { return false }

        var bytes: [UInt8]?
        if merged, let frame = exportPixelsFromGPU(width: width, height: height) {
            bytes = cropPixels(frame, width: width, region: region)
        }
        if bytes == nil {
            // No resident frame to copy from: bring the core up to date and compose there.
            materializeCanvas()
            var read = [UInt8](repeating: 0, count: count)
            let found = read.withUnsafeMutableBytes { buffer -> Bool in
                let out = buffer.bindMemory(to: UInt8.self).baseAddress
                if merged {
                    return dk_composite_rect(
                        handle, UInt32(region.x), UInt32(region.y),
                        UInt32(region.width), UInt32(region.height), out) != 0
                }
                return dk_layer_rect(
                    handle, UInt32(activeLayer), UInt32(region.x), UInt32(region.y),
                    UInt32(region.width), UInt32(region.height), out) != 0
            }
            guard found else { return false }
            bytes = read
        }
        guard let pixels = bytes,
            let image = clipboardImage(pixels, width: region.width, height: region.height)
        else { return false }

        let rep = NSBitmapImageRep(cgImage: image)
        guard let tiff = rep.tiffRepresentation,
            let png = rep.representation(using: .png, properties: [:])
        else { return false }
        let board = NSPasteboard.general
        board.clearContents()
        board.setData(tiff, forType: .tiff)
        board.setData(png, forType: .png)
        return true
    }

    /// Copy `region` out of a whole-canvas premultiplied frame, row by row.
    private func cropPixels(_ canvas: [UInt8], width: Int, region: SurfaceRect) -> [UInt8]? {
        guard region.x >= 0, region.y >= 0,
            region.x + region.width <= width, region.y + region.height <= height,
            canvas.count >= width * height * 4
        else { return nil }
        let rowBytes = region.width * 4
        var out = [UInt8](repeating: 0, count: rowBytes * region.height)
        out.withUnsafeMutableBytes { target in
            canvas.withUnsafeBytes { source in
                guard let destination = target.baseAddress, let pixels = source.baseAddress
                else { return }
                for row in 0..<region.height {
                    let from = ((region.y + row) * width + region.x) * 4
                    memcpy(destination + row * rowBytes, pixels + from, rowBytes)
                }
            }
        }
        return out
    }

    /// Copy, then empty the selection (or the layer when nothing is selected).
    @discardableResult
    func cutPixels() -> Bool {
        guard copyPixels() else { return false }
        clear()
        return true
    }

    /// Paste the pasteboard as a new layer. It lands at the selection's corner, or centred when
    /// there is no selection; nothing is scaled, so a 1:1 copy stays 1:1.
    @discardableResult
    func pastePixels() -> Bool {
        let board = NSPasteboard.general
        let data = board.data(forType: .tiff) ?? board.data(forType: .png)
        guard let data, let rep = NSBitmapImageRep(data: data), let source = rep.cgImage
        else { return false }
        let sourceWidth = source.width
        let sourceHeight = source.height
        guard sourceWidth > 0, sourceHeight > 0 else { return false }

        let originX = selection.map { $0.x } ?? (width - sourceWidth) / 2
        let originY = selection.map { $0.y } ?? (height - sourceHeight) / 2
        guard let buffer = pasteBuffer(source, originX: originX, originY: originY) else {
            return false
        }
        guard addImageLayer(premultiplied: buffer, name: String(localized: "Pasted")) else {
            return false
        }
        return true
    }

    /// A premultiplied CGImage over the working colour space, for the pasteboard.
    private func clipboardImage(_ pixels: [UInt8], width: Int, height: Int) -> CGImage? {
        guard let provider = CGDataProvider(data: Data(pixels) as CFData) else { return nil }
        return CGImage(
            width: width, height: height, bitsPerComponent: 8, bitsPerPixel: 32,
            bytesPerRow: width * 4, space: colorSpace.cgColorSpace,
            bitmapInfo: CGBitmapInfo(rawValue: CGImageAlphaInfo.premultipliedLast.rawValue),
            provider: provider, decode: nil, shouldInterpolate: false, intent: .defaultIntent)
    }

    /// Draw the pasted image into a canvas-sized premultiplied buffer. The bitmap context has its
    /// origin at the bottom left while the layer rows run top down, so the y is flipped once.
    private func pasteBuffer(_ source: CGImage, originX: Int, originY: Int) -> [UInt8]? {
        var buffer = [UInt8](repeating: 0, count: width * height * 4)
        let drawn = buffer.withUnsafeMutableBytes { bytes -> Bool in
            guard
                let context = CGContext(
                    data: bytes.baseAddress, width: width, height: height, bitsPerComponent: 8,
                    bytesPerRow: width * 4, space: colorSpace.cgColorSpace,
                    bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)
            else { return false }
            context.interpolationQuality = .none
            context.draw(
                source,
                in: CGRect(
                    x: originX, y: height - originY - source.height,
                    width: source.width, height: source.height))
            return true
        }
        return drawn ? buffer : nil
    }
}
