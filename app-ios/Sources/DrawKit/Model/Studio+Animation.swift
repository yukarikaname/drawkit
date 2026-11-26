// Animation frames on iPad: the core owns whole-document snapshots and their tapes, the sheet
// owns thumbnails, playback and onion skin.

import SwiftUI
import UIKit
import CoreGraphics

extension Studio {
    func refreshFrames() {
        guard handle != nil else { return }
        // Frame composites read the core cache, which a GPU stroke leaves behind.
        materializeCanvas()
        frameCount = max(Int(dk_frame_count(handle)), 1)
        activeFrame = Int(dk_frame_active(handle))
        fps = Int(dk_fps(handle))
        frameDuration = Int(dk_frame_duration(handle))
        loopEnabled = dk_loop_enabled(handle) != 0
        loopStart = Int(dk_loop_start(handle))
        loopEnd = Int(dk_loop_end(handle))
        var thumbnails: [UIImage?] = []
        thumbnails.reserveCapacity(frameCount)
        for index in 0..<frameCount {
            var pixels = [UInt8](repeating: 0, count: width * height * 4)
            let drawn = pixels.withUnsafeMutableBytes {
                dk_frame_composite(handle, UInt32(index),
                                   $0.bindMemory(to: UInt8.self).baseAddress)
            }
            let image = drawn != 0 ? frameImage(pixels) : nil
            thumbnails.append(image.map { UIImage(cgImage: $0) })
        }
        frameThumbnails = thumbnails
        refreshOnion()
        revision += 1
    }

    func refreshOnion() {
        guard handle != nil, onionEnabled, frameCount > 1 else {
            onionImage = nil
            revision += 1
            return
        }
        var pixels = [UInt8](repeating: 0, count: width * height * 4)
        let drawn = pixels.withUnsafeMutableBytes {
            dk_onion_composite(handle, UInt32(onionBefore), UInt32(onionAfter),
                               $0.bindMemory(to: UInt8.self).baseAddress)
        }
        onionImage = drawn != 0 ? frameImage(pixels) : nil
        revision += 1
    }

    func selectFrame(_ index: Int, refresh: Bool = true) {
        if refresh { stopPlayback() }
        // Switching replaces the whole layer set; resident tiles must reach the core first.
        materializeCanvas()
        guard dk_frame_select(handle, UInt32(index)) != 0 else { return }
        refreshAll()
        refreshLayers()
        if refresh {
            refreshFrames()
        } else {
            activeFrame = index
            revision += 1
        }
    }

    func stepFrame(_ delta: Int) {
        stopPlayback()
        let (start, end) = loopRange()
        var next: Int
        if activeFrame < start || activeFrame >= end {
            next = delta >= 0 ? start : end - 1
        } else {
            next = activeFrame + delta
            if next >= end { next = start }
            if next < start { next = end - 1 }
        }
        selectFrame(next)
    }

    func addFrame(duplicate: Bool) {
        stopPlayback()
        // A duplicate copies the core cache, so any resident stroke has to be in it.
        materializeCanvas()
        guard dk_frame_add(handle, duplicate ? 1 : 0) >= 0 else { return }
        refreshAll()
        refreshLayers()
        refreshFrames()
        finishChange()
    }

    func removeFrame() {
        stopPlayback()
        materializeCanvas()
        guard dk_frame_remove(handle, UInt32(activeFrame)) != 0 else { return }
        refreshAll()
        refreshLayers()
        refreshFrames()
        finishChange()
    }

    func togglePlayback() {
        if isPlaying {
            stopPlayback()
        } else {
            startPlayback()
        }
    }

    func startPlayback() {
        guard frameCount > 1, playbackTask == nil else { return }
        isPlaying = true
        onionImage = nil
        revision += 1
        playbackTask = Task { @MainActor [weak self] in
            while !Task.isCancelled {
                guard let self else { break }
                let hold = max(Int(dk_frame_duration(handle)), 10)
                try? await Task.sleep(nanoseconds: UInt64(hold) * 1_000_000)
                if Task.isCancelled { break }
                let (start, end) = loopRange()
                var next = activeFrame + 1
                if next >= end {
                    guard loopEnabled else {
                        stopPlayback()
                        break
                    }
                    next = start
                }
                selectFrame(next, refresh: false)
            }
        }
    }

    func stopPlayback() {
        guard isPlaying || playbackTask != nil else { return }
        playbackTask?.cancel()
        playbackTask = nil
        isPlaying = false
        refreshFrames()
    }

    func loopRange() -> (Int, Int) {
        let count = max(frameCount, 1)
        let start = min(max(loopStart, 0), count - 1)
        let end = (loopEnd <= 0 || loopEnd > count) ? count : max(loopEnd, start + 1)
        return (start, end)
    }

    private func frameImage(_ pixels: [UInt8]) -> CGImage? {
        guard let provider = CGDataProvider(data: Data(pixels) as CFData) else { return nil }
        return CGImage(
            width: width, height: height, bitsPerComponent: 8, bitsPerPixel: 32,
            bytesPerRow: width * 4, space: CGColorSpaceCreateDeviceRGB(),
            bitmapInfo: CGBitmapInfo(rawValue: CGImageAlphaInfo.premultipliedLast.rawValue),
            provider: provider, decode: nil, shouldInterpolate: false, intent: .defaultIntent
        )
    }
}
