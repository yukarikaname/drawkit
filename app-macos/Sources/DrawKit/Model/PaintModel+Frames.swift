// Animation frames are whole-document snapshots in the core. The shell only needs the active
// index, a thumbnail per frame, and the tinted onion composite.

import CoreGraphics
import Foundation

extension PaintModel {
    func refreshFrames() {
        guard handle != nil else { return }
        let count = max(Int(dk_frame_count(handle)), 1)
        frameCount = count
        activeFrame = Int(dk_frame_active(handle))
        var images: [CGImage?] = []
        for index in 0..<count {
            let buffer = PixelBuffer(count: width * height * 4)
            let drawn = dk_frame_composite(handle, UInt32(index), buffer.pointer)
            images.append(drawn != 0 ? makeImage(buffer: buffer) : nil)
        }
        frameImages = images
        refreshOnion()
        revision &+= 1
    }

    func addFrame(duplicate: Bool) {
        guard dk_frame_add(handle, duplicate ? 1 : 0) >= 0 else { return }
        syncLayers()
        redrawAll()
        refreshFrames()
    }

    func selectFrame(_ index: Int) {
        guard dk_frame_select(handle, UInt32(index)) != 0 else { return }
        cancelFillAnimation()
        syncLayers()
        redrawAll()
        refreshFrames()
    }

    func removeFrame() {
        guard dk_frame_remove(handle, UInt32(activeFrame)) != 0 else { return }
        syncLayers()
        redrawAll()
        refreshFrames()
    }

    func refreshOnion() {
        guard handle != nil, onionEnabled, frameCount > 1 else {
            if onionImage != nil { onionImage = nil; revision &+= 1 }
            return
        }
        let buffer = PixelBuffer(count: width * height * 4)
        let drawn = dk_onion_composite(handle, UInt32(onionBefore), UInt32(onionAfter),
                                       buffer.pointer)
        onionImage = drawn != 0 ? makeImage(buffer: buffer) : nil
        revision &+= 1
    }
}
