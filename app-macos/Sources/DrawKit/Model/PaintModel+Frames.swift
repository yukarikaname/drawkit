// Animation frames are whole-document snapshots in the core. The shell only needs the active
// index, a thumbnail per frame, and the tinted onion composite.

import AppKit
import CoreGraphics
import UniformTypeIdentifiers
import Foundation

extension PaintModel {
    func refreshFrames() {
        guard handle != nil else { return }
        let count = max(Int(dk_frame_count(handle)), 1)
        frameCount = count
        activeFrame = Int(dk_frame_active(handle))
        fps = Int(dk_fps(handle))
        frameDuration = Int(dk_frame_duration(handle))
        loopEnabled = dk_loop_enabled(handle) != 0
        loopStart = Int(dk_loop_start(handle))
        loopEnd = Int(dk_loop_end(handle))
        var images: [CGImage?] = []
        // The strip draws 44 px frames; a full-canvas image per frame is 268 MB at 8K.
        let size = 64
        for index in 0..<count {
            let buffer = PixelBuffer(count: size * size * 4)
            let drawn = dk_frame_thumbnail(handle, UInt32(index), UInt32(size), UInt32(size),
                                           buffer.pointer)
            images.append(drawn != 0
                ? makeImage(buffer: buffer, width: size, height: size) : nil)
        }
        frameImages = images
        refreshOnion()
        revision &+= 1
    }

    /// One frame changed while painting; only its own strip thumbnail is stale.
    func refreshActiveFrameThumbnail() {
        guard activeFrame < frameImages.count else { return }
        let size = 64
        let buffer = PixelBuffer(count: size * size * 4)
        if dk_frame_thumbnail(handle, UInt32(activeFrame), UInt32(size), UInt32(size),
                              buffer.pointer) != 0 {
            frameImages[activeFrame] = makeImage(buffer: buffer, width: size, height: size)
        }
        revision &+= 1
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

    /// The loop range the core will use, clamped to the frames that exist.
    func loopRange() -> (Int, Int) {
        let count = max(frameCount, 1)
        let start = min(max(loopStart, 0), count - 1)
        let end = (loopEnd <= 0 || loopEnd > count) ? count : max(loopEnd, start + 1)
        return (start, end)
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
        stopFillAnimationForPlayback()
        isPlaying = true
        playbackTask = Task { @MainActor [weak self] in
            while !Task.isCancelled {
                guard let self else { break }
                let hold = max(Int(dk_frame_duration(handle)), 10)
                try? await Task.sleep(nanoseconds: UInt64(hold) * 1_000_000)
                if Task.isCancelled { break }
                let (start, end) = self.loopRange()
                var next = self.activeFrame + 1
                if next >= end {
                    guard self.loopEnabled else {
                        self.stopPlayback()
                        break
                    }
                    next = start
                }
                self.selectFrame(next, refresh: false)
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

    private func stopFillAnimationForPlayback() {
        if fillTask != nil { finishFillAnimation() }
    }

    func addFrame(duplicate: Bool) {
        stopPlayback()
        guard dk_frame_add(handle, duplicate ? 1 : 0) >= 0 else { return }
        syncLayers()
        redrawAll()
        refreshFrames()
    }

    func selectFrame(_ index: Int, refresh: Bool = true) {
        if refresh { stopPlayback() }
        guard dk_frame_select(handle, UInt32(index)) != 0 else { return }
        cancelFillAnimation()
        syncLayers()
        redrawAll()
        if refresh {
            refreshFrames()
        } else {
            activeFrame = index
            revision &+= 1
        }
    }

    func removeFrame() {
        stopPlayback()
        guard dk_frame_remove(handle, UInt32(activeFrame)) != 0 else { return }
        syncLayers()
        redrawAll()
        refreshFrames()
    }

    func exportAnimation() {
        let panel = NSOpenPanel()
        panel.canChooseDirectories = true
        panel.canChooseFiles = false
        panel.allowsMultipleSelection = false
        panel.prompt = "Export"
        guard panel.runModal() == .OK, let url = panel.url else { return }
        let saved = url.path.withCString { dk_export_animation(handle, $0) }
        if saved != 0 {
            flash(String(localized: "Exported PNG sequence"))
        } else {
            NSSound.beep()
            flash(String(localized: "Could not export animation"), error: true)
        }
    }

    func exportAseprite() {
        let panel = NSSavePanel()
        panel.allowedContentTypes = [UTType(filenameExtension: "aseprite") ?? .data]
        panel.nameFieldStringValue = "animation.aseprite"
        guard panel.runModal() == .OK, let url = panel.url else { return }
        let saved = url.path.withCString { dk_export_aseprite(handle, $0) }
        if saved != 0 {
            flash(String(localized: "Exported Aseprite"))
        } else {
            NSSound.beep()
            flash(String(localized: "Could not export Aseprite"), error: true)
        }
    }

    func importAnimation() {
        let panel = NSOpenPanel()
        panel.canChooseDirectories = true
        panel.canChooseFiles = false
        panel.allowsMultipleSelection = false
        panel.prompt = "Import"
        guard panel.runModal() == .OK, let url = panel.url else { return }
        let imported = url.path.withCString { dk_import_animation(handle, $0) }
        guard imported != 0 else {
            NSSound.beep()
            flash(String(localized: "Could not import animation"), error: true)
            return
        }
        adoptImportedGeometry()
        pixels = PixelBuffer(count: width * height * 4)
        linearPixels = needsLinear ? LinearBuffer(count: width * height * 4) : nil
        markGPURebuild()
        markSurfaceFull()
        syncLayers()
        redrawAll()
        refreshThumbnails()
        refreshFrames()
        documentURL = nil
        isDirty = true
        revision &+= 1
        flash(String(localized: "Imported PNG sequence"))
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
