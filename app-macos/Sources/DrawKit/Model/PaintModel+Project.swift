// Autosave, recovery and project files. The model owns the document; this file owns where it
// lives on disk and how a background write avoids the pixels the painter is touching.

import SwiftUI
import AppKit
import UniformTypeIdentifiers

/// One layer of the resident stack, downsampled for the palette. The palette used to read the
/// core's CPU surface; this reads the tiles the strokes actually land in.
@_silgen_name("dk_canvas_surface_layer_thumbnail")
private func dk_canvas_surface_layer_thumbnail(
    _ handle: UnsafeMutableRawPointer?, _ layer: UInt32,
    _ width: UInt32, _ height: UInt32, _ out: UnsafeMutablePointer<UInt8>?) -> Int32

extension PaintModel {
    // MARK: Autosave

    /// Write one recovery copy if there are unsaved edits. The temporary file is renamed over the
    /// previous copy, so a crash during the write leaves the last good recovery intact.
    ///
    /// The copy carries the pixel cache, like a normal save, so recovery does not have to replay
    /// thousands of strokes - and an imported layer that was painted on keeps its base pixels
    /// instead of losing them. Encoding a large canvas in the background is what makes that
    /// affordable.
    func autosaveIfNeeded() {
        // Benchmarks run with `DRAWKIT_BENCH`; never leave a recovery copy behind for them.
        guard !CanvasView.automatedRun else { return }
        flushDeferred()
        guard isDirty, !autosaving, !autosaveBlockedUntilEdit else { return }
        let directory = Autosave.directory
        try? FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        let temporary = Autosave.documentURL.appendingPathExtension("tmp")
        try? FileManager.default.removeItem(at: temporary)
        guard let snapshot = dk_editor_clone(handle) else { return }
        let job = SaveJob(handle: snapshot, path: temporary.path)
        let record = AutosaveRecord(savedAt: Date(), source: documentURL?.path,
                                    width: width, height: height)
        let epoch = autosaveEpoch
        autosaving = true
        Task.detached(priority: .utility) {
            let saved = job.path.withCString { dk_save_tape(job.handle, $0) }
            dk_editor_free(job.handle)
            await MainActor.run { [weak self] in
                guard let self else { return }
                self.autosaving = false
                guard saved != 0, self.autosaveEpoch == epoch else {
                    try? FileManager.default.removeItem(at: temporary)
                    return
                }
                if FileManager.default.fileExists(atPath: Autosave.documentURL.path) {
                    _ = try? FileManager.default.replaceItemAt(Autosave.documentURL,
                                                               withItemAt: temporary)
                } else {
                    try? FileManager.default.moveItem(at: temporary, to: Autosave.documentURL)
                }
                if let data = try? JSONEncoder().encode(record) {
                    try? data.write(to: Autosave.recordURL, options: .atomic)
                }
                self.lastAutosave = Date()
            }
        }
    }

    /// Drop the recovery copy and invalidate any autosave still running against the old one.
    func discardAutosave() {
        autosaveEpoch &+= 1
        autosaveBlockedUntilEdit = true
        Autosave.clear()
    }

    /// Load the recovery copy; the original file name is kept for a later "Save As".
    func recover(_ record: AutosaveRecord) {
        guard openDocument(Autosave.documentURL, recovered: true) else { return }
        if let source = record.source {
            documentURL = URL(fileURLWithPath: source)
        }
        isDirty = true
        autosaveBlockedUntilEdit = false
    }

    // MARK: Project files

    /// Save to the current path when there is one; otherwise ask for a path.
    func saveProject() {
        flushDeferred()
        if let url = documentURL {
            Backups.archive(url)
            saveProjectInBackground(to: url)
            return
        }
        saveProjectAs()
    }

    func saveProjectAs() {
        flushDeferred()
        let panel = NSSavePanel()
        panel.allowedContentTypes = [UTType(filenameExtension: "dkp") ?? .data]
        panel.nameFieldStringValue = "painting.dkp"
        guard panel.runModal() == .OK, let url = panel.url else { return }
        let folder = url.deletingLastPathComponent()
        guard FileManager.default.isWritableFile(atPath: folder.path) else {
            NSSound.beep()
            flash(String(localized: "Cannot write to \(folder.lastPathComponent): check the folder permissions"),
                  error: true)
            return
        }
        Backups.archive(url)
        saveProjectInBackground(to: url)
    }

    /// Clone the editor and let a worker thread encode the layer caches. An 8K painting takes
    /// seconds to write; the canvas stays live and the strokes made meanwhile keep the document
    /// dirty instead of being reported as saved.
    private func saveProjectInBackground(to url: URL) {
        guard let snapshot = dk_editor_clone(handle) else {
            NSSound.beep()
            flash(String(localized: "Could not save"), error: true)
            return
        }
        let mark = editCount
        let epoch = documentEpoch
        let job = SaveJob(handle: snapshot, path: url.path)
        flash(String(localized: "Saving…"))
        Task.detached(priority: .userInitiated) {
            let saved = job.path.withCString { dk_save_tape(job.handle, $0) }
            dk_editor_free(job.handle)
            await MainActor.run { [weak self] in
                guard let self else { return }
                guard saved != 0 else {
                    NSSound.beep()
                    self.flash(String(localized: "Could not save \(url.lastPathComponent): check disk space and permissions"),
                               error: true)
                    return
                }
                // A document opened while the write ran keeps its own name and dirty state.
                guard self.documentEpoch == epoch else { return }
                self.documentURL = url
                RecentDocuments.add(url)
                self.cacheThumbnail(for: url)
                if self.editCount == mark {
                    self.isDirty = false
                    self.discardAutosave()
                }
                self.flash(String(localized: "Saved"))
            }
        }
    }

    /// Ask the person for a project or image, then open it at its own size.
    @discardableResult
    func openProject() -> Bool {
        let panel = NSOpenPanel()
        panel.allowedContentTypes = ["dkp", "kra", "psd", "png", "jpg", "jpeg", "webp"]
            .compactMap { UTType(filenameExtension: $0) }
        panel.allowsMultipleSelection = false
        guard panel.runModal() == .OK, let url = panel.url else { return false }
        return openDocument(url)
    }

    func cacheThumbnail(for url: URL) {
        if image == nil || gpuEligible {
            refreshCPUImage()
        }
        guard let image else { return }
        RecentDocuments.storeThumbnail(image, for: url)
    }

    /// Thumbnails are the expensive part of a layer change, not the stroke. Coalesce the updates
    /// so a fast series of strokes does not rebuild every layer preview for each one.
    func scheduleThumbnails() {
        thumbnailTask?.cancel()
        thumbnailTask = Task { @MainActor [weak self] in
            try? await Task.sleep(for: .milliseconds(220))
            guard !Task.isCancelled else { return }
            self?.refreshThumbnails()
        }
    }

    func refreshThumbnails() {
        isBlank = dk_is_clear(handle) != 0
        refreshOverview()
        // A rebuild (a new document, a layer change) means the resident stack still holds the
        // previous painting. The upload schedules another refresh, so show nothing rather than
        // the wrong rows for a frame.
        guard !gpuNeedsRebuild, let surfaceHandle = paintSurface?.handle else {
            thumbnails = layers.map { _ in nil }
            return
        }
        let size = 40
        var images: [CGImage?] = layers.indices.map { index in
            layers[index].kind == .group
                ? nil
                : residentThumbnail(index: index, size: size, surfaceHandle: surfaceHandle)
        }
        // The resident stack draws its own previews, so the CPU cache does not have to be current.
        // Only a layer the GPU refuses (extended range, a fill the stack cannot draw) falls back
        // to the core, and only that case pays for a materialise.
        let failed = layers.indices.contains {
            images[$0] == nil && layers[$0].kind != .group
        }
        if failed {
            materializeCanvas()
            for index in layers.indices where images[index] == nil && layers[index].kind != .group {
                images[index] = coreThumbnail(index: index, size: size)
            }
        }
        thumbnails = images
    }

    private func residentThumbnail(index: Int, size: Int,
                                   surfaceHandle: UnsafeMutableRawPointer) -> CGImage? {
        var buffer = [UInt8](repeating: 0, count: size * size * 4)
        let drawn = buffer.withUnsafeMutableBytes {
            dk_canvas_surface_layer_thumbnail(
                surfaceHandle, UInt32(index), UInt32(size), UInt32(size),
                $0.bindMemory(to: UInt8.self).baseAddress)
        }
        guard drawn != 0 else { return nil }
        return thumbnailImage(buffer: buffer, size: size)
    }

    private func coreThumbnail(index: Int, size: Int) -> CGImage? {
        var buffer = [UInt8](repeating: 0, count: size * size * 4)
        let drawn = buffer.withUnsafeMutableBytes {
            dk_layer_thumbnail(handle, UInt32(index), UInt32(size), UInt32(size),
                               $0.bindMemory(to: UInt8.self).baseAddress)
        }
        guard drawn != 0 else { return nil }
        return thumbnailImage(buffer: buffer, size: size)
    }

    private func thumbnailImage(buffer: [UInt8], size: Int) -> CGImage? {
        guard let provider = CGDataProvider(data: Data(buffer) as CFData) else { return nil }
        return CGImage(
            width: size, height: size, bitsPerComponent: 8, bitsPerPixel: 32,
            bytesPerRow: size * 4, space: colorSpace.cgColorSpace,
            bitmapInfo: CGBitmapInfo(rawValue: CGImageAlphaInfo.premultipliedLast.rawValue),
            provider: provider, decode: nil, shouldInterpolate: false,
            intent: .defaultIntent)
    }

    private func refreshOverview() {
        let maxSide = 192
        let scale = min(1.0, Double(maxSide) / Double(max(width, height, 1)))
        let overviewWidth = max(1, Int(Double(width) * scale))
        let overviewHeight = max(1, Int(Double(height) * scale))
        guard let pixels = paintSurface?.thumbnail(width: overviewWidth, height: overviewHeight),
              let provider = CGDataProvider(data: Data(pixels) as CFData),
              let image = CGImage(
                width: overviewWidth, height: overviewHeight,
                bitsPerComponent: 8, bitsPerPixel: 32,
                bytesPerRow: overviewWidth * 4, space: colorSpace.cgColorSpace,
                bitmapInfo: CGBitmapInfo(rawValue: CGImageAlphaInfo.premultipliedLast.rawValue),
                provider: provider, decode: nil, shouldInterpolate: true,
                intent: .defaultIntent)
        else { return }
        overview = image
    }
}

/// One project write on a worker thread. The pointer owns a cloned editor, so the job may cross
/// to another thread and is freed there.
private struct SaveJob: @unchecked Sendable {
    let handle: UnsafeMutableRawPointer
    let path: String
}
