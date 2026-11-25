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

    func stepFrame(_ delta: Int) {
        stopPlayback()
        let count = max(frameCount, 1)
        selectFrame((activeFrame + delta + count) % count)
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
                let interval = UInt64(1_000_000_000 / max(self.fps, 1))
                try? await Task.sleep(nanoseconds: interval)
                if Task.isCancelled { break }
                let count = max(self.frameCount, 1)
                self.selectFrame((self.activeFrame + 1) % count, refresh: false)
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
            flash("Exported PNG sequence")
        } else {
            NSSound.beep()
            flash("Could not export animation", error: true)
        }
    }

    func exportAseprite() {
        let panel = NSSavePanel()
        panel.allowedContentTypes = [UTType(filenameExtension: "aseprite") ?? .data]
        panel.nameFieldStringValue = "animation.aseprite"
        guard panel.runModal() == .OK, let url = panel.url else { return }
        let saved = url.path.withCString { dk_export_aseprite(handle, $0) }
        if saved != 0 {
            flash("Exported Aseprite")
        } else {
            NSSound.beep()
            flash("Could not export Aseprite", error: true)
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
            flash("Could not import animation", error: true)
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
        flash("Imported PNG sequence")
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
