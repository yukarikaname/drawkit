// Export and import panels. They live on the model because each one owns a snapshot or a file,
// not because the window has to know how a PNG or an MP4 is written.

import SwiftUI
import AppKit
import UniformTypeIdentifiers

extension PaintModel {
    // MARK: Export

    /// Render the stroke tape to an MP4. The tape holds the timestamps, so the video is a view of
    /// the painting rather than a screen capture; the encoder runs against a snapshot off the main
    /// thread and painting can continue.
    func exportTimelapse() {
        guard !timelapseExporting else { return }
        let duration = dk_recording_ms(handle)
        guard dk_recording_events(handle) > 0 else {
            NSSound.beep()
            flash("There is nothing on the tape to record", error: true)
            return
        }
        let panel = NSSavePanel()
        panel.allowedContentTypes = [.mpeg4Movie]
        panel.nameFieldStringValue = "timelapse.mp4"
        guard panel.runModal() == .OK, let url = panel.url else { return }
        guard let snapshot = dk_editor_clone(handle) else {
            NSSound.beep()
            flash("Could not start the video", error: true)
            return
        }
        let width = width
        let height = height
        let job = SnapshotJob(handle: snapshot)
        timelapseExporting = true
        flash("Rendering timelapse…")
        Task.detached(priority: .userInitiated) { [weak self] in
            do {
                var reported = -10
                try await Timelapse.write(
                    handle: job.handle,
                    durationMs: max(duration, 1),
                    canvasWidth: width,
                    canvasHeight: height,
                    to: url
                ) { fraction in
                    let percent = Int(fraction * 100)
                    guard percent - reported >= 10 else { return }
                    reported = percent
                    Task { @MainActor [weak self] in
                        self?.flash("Rendering timelapse… \(percent)%")
                    }
                }
            } catch {
                dk_editor_free(job.handle)
                await MainActor.run { [weak self] in
                    guard let self else { return }
                    self.timelapseExporting = false
                    NSSound.beep()
                    self.flash(error.localizedDescription, error: true)
                }
                return
            }
            dk_editor_free(job.handle)
            await MainActor.run { [weak self] in
                guard let self else { return }
                self.timelapseExporting = false
                self.flash("Exported \(url.lastPathComponent)")
            }
        }
    }

    /// Save what the canvas shows. The document is transparent where nothing is drawn, but the
    /// painter sees paper, so the file gets the paper rather than a checkerboard.
    func exportPNG() {
        if previewMs != nil {
            previewMs = nil
            refresh()
        }
        guard let url = askForURL() else { return }
        var rgba = [UInt8](repeating: 0, count: width * height * 4)
        let drawn = rgba.withUnsafeMutableBytes { buffer in
            dk_composite_export(handle, buffer.bindMemory(to: UInt8.self).baseAddress)
        }
        guard drawn != 0,
              let provider = CGDataProvider(data: Data(rgba) as CFData),
              let image = CGImage(
                width: width, height: height, bitsPerComponent: 8, bitsPerPixel: 32,
                bytesPerRow: width * 4, space: colorSpace.cgColorSpace,
                bitmapInfo: CGBitmapInfo(rawValue: CGImageAlphaInfo.premultipliedLast.rawValue),
                provider: provider, decode: nil, shouldInterpolate: true,
                intent: .defaultIntent)
        else { return }
        guard let context = CGContext(
            data: nil, width: image.width, height: image.height, bitsPerComponent: 8,
            bytesPerRow: 0, space: colorSpace.cgColorSpace,
            bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue) else { return }
        let bounds = CGRect(x: 0, y: 0, width: image.width, height: image.height)
        context.setFillColor(NSColor.white.cgColor)
        context.fill(bounds)
        context.draw(image, in: bounds)
        guard let flattened = context.makeImage(),
              let destination = CGImageDestinationCreateWithURL(
                url as CFURL, UTType.png.identifier as CFString, 1, nil)
        else { return }
        CGImageDestinationAddImage(destination, flattened, nil)
        if CGImageDestinationFinalize(destination) {
            flash("Exported")
        } else {
            NSSound.beep()
            flash("Could not export", error: true)
        }
    }

    /// Export the canvas as linear OpenEXR, the format that keeps highlights above paper white.
    /// One PNG per paint or fill layer, named in palette order.
    /// Turn an imported layer of ink on white paper into transparent ink. Painted layers are
    /// refused because their colour is already the ink, not ink composited over paper.
    func removePaperFromLayer() {
        if dk_remove_paper(handle) != 0 {
            edited()
            refresh()
            refreshThumbnails()
            flash("Removed the paper from this layer")
        } else {
            NSSound.beep()
            flash("Only an imported layer without strokes can lose its paper", error: true)
        }
    }

    func exportLayers() {
        let panel = NSOpenPanel()
        panel.canChooseDirectories = true
        panel.canChooseFiles = false
        panel.canCreateDirectories = true
        panel.prompt = "Export Layers"
        panel.message = "Choose a folder for one PNG per layer"
        guard panel.runModal() == .OK, let url = panel.url else { return }
        let count = url.path.withCString { dk_export_layers(handle, $0) }
        if count > 0 {
            flash("Exported \(count) layer\(count == 1 ? "" : "s")")
        } else {
            NSSound.beep()
            flash("Could not export layers", error: true)
        }
    }

    func exportJPEG() {
        exportDocument(extension: "jpg", name: "drawkit.jpg") { dk_export_jpeg(handle, $0, 92) }
    }

    func exportWebP() {
        exportDocument(extension: "webp", name: "drawkit.webp") { dk_export_webp(handle, $0) }
    }

    func exportEXR() {
        if previewMs != nil {
            previewMs = nil
            refresh()
        }
        let panel = NSSavePanel()
        panel.nameFieldStringValue = "drawkit.exr"
        guard panel.runModal() == .OK, let url = panel.url else { return }
        if url.path.withCString({ dk_export_exr(handle, $0) }) != 0 {
            flash("Exported")
        } else {
            NSSound.beep()
            flash("Could not export", error: true)
        }
    }

    /// Say what happened, then get out of the way.
    func flash(_ text: String, error: Bool = false) {
        statusTask?.cancel()
        withAnimation(Motion.panel.base) { status = StatusMessage(text: text, isError: error) }
        statusTask = Task { @MainActor [weak self] in
            try? await Task.sleep(for: .seconds(2))
            guard !Task.isCancelled else { return }
            withAnimation(Motion.panel.base) { self?.status = nil }
        }
    }

    /// Replace the painting with a Krita or Photoshop document.
    func importDocument() {
        let panel = NSOpenPanel()
        panel.allowedContentTypes = ["kra", "psd"].compactMap { UTType(filenameExtension: $0) }
        panel.allowsMultipleSelection = false
        guard panel.runModal() == .OK, let url = panel.url else { return }
        let imported = url.path.withCString { path -> Int32 in
            switch url.pathExtension.lowercased() {
            case "kra": dk_import_kra(handle, path)
            case "psd": dk_import_psd(handle, path)
            default: 0
            }
        }
        guard imported != 0 else {
            NSSound.beep()
            flash("Could not import", error: true)
            return
        }
        edited()
        recordingMs = 0
        previewMs = nil
        syncLayers()
        redrawAll()
        scheduleThumbnails()
        flash("Imported \(url.lastPathComponent)")
    }

    func exportKra() { exportDocument(extension: "kra", name: "painting.kra") { dk_export_kra(handle, $0) } }
    func exportPsd() { exportDocument(extension: "psd", name: "painting.psd") { dk_export_psd(handle, $0) } }

    private func exportDocument(extension pathExtension: String, name: String,
                                writer: (UnsafePointer<CChar>) -> Int32) {
        let panel = NSSavePanel()
        panel.allowedContentTypes = [UTType(filenameExtension: pathExtension) ?? .data]
        panel.nameFieldStringValue = name
        guard panel.runModal() == .OK, let url = panel.url else { return }
        let saved = url.path.withCString { writer($0) }
        if saved != 0 {
            flash("Exported \(url.lastPathComponent)")
        } else {
            NSSound.beep()
            flash("Could not export", error: true)
        }
    }

    /// Save the rich stroke tape. Recording must be armed; an empty tape is not a file.
    func saveTape() {
        let panel = NSSavePanel()
        panel.allowedContentTypes = [UTType(filenameExtension: "dkp") ?? .data]
        panel.nameFieldStringValue = "painting.dkp"
        guard panel.runModal() == .OK, let url = panel.url else { return }
        let saved = url.path.withCString { dk_save_tape(handle, $0) }
        if saved != 0 {
            flash("Saved stroke tape")
        } else {
            NSSound.beep()
            flash("No strokes on the tape yet", error: true)
        }
    }

    private func askForURL() -> URL? {
        let panel = NSSavePanel()
        panel.allowedContentTypes = [.png]
        panel.nameFieldStringValue = "drawkit.png"
        return panel.runModal() == .OK ? panel.url : nil
    }
}

/// An editor snapshot that may cross to a worker thread. The worker owns the pointer and frees it.
private struct SnapshotJob: @unchecked Sendable {
    let handle: UnsafeMutableRawPointer
}
