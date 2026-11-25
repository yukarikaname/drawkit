// The document model: one core editor, its layers, its tape, and the image the canvas draws.
//
// The shell never keeps pixels of its own. It owns one buffer and lets the core write into it; the
// image the view draws is that buffer, not a copy. A stroke asks for the rectangle it dirtied and
// uploads only that.

import SwiftUI
import AppKit
import Observation
import UniformTypeIdentifiers
import ImageIO

private final class CanvasHandle {
    var pointer: UnsafeMutableRawPointer?

    deinit {
        dk_editor_free(pointer)
    }
}

@MainActor
@Observable
final class PaintModel {
    var image: CGImage?
    /// Brush parameters exposed to the brush studio, in the core's parameter order.
    var brushParameters = [Double](repeating: 0, count: 15)
    var brushPreview: CGImage?
    /// One small preview per preset slot, painted by the core.
    var brushSlotPreviews: [CGImage?] = Array(repeating: nil, count: 9)
    var brushTextureImage: CGImage?
    /// A GPU stroke waiting for its bounded CPU commit.
    var deferredPending = false
    /// The resident tiles hold pixels the CPU cache has not seen.
    @ObservationIgnored var gpuStale = false
    /// The canvas surface, set by the view that owns it.
    @ObservationIgnored weak var paintSurface: PaintSurface?
    var recordingMs: UInt64 = 0
    /// When set, the canvas shows the tape up to this millisecond instead of the document.
    var previewMs: UInt64? {
        didSet {
            // A replay surface keeps a second copy of the layers resident; release it as soon as
            // the timeline leaves the preview.
            if previewMs == nil || oldValue == nil {
                tapeReplay = nil
                tapeReplayUnavailable = false
            }
        }
    }
    /// The headless GPU replay used by the timeline, when the tape can run on it.
    @ObservationIgnored var tapeReplay: TapeReplay?
    @ObservationIgnored var tapeReplayUnavailable = false
    var layers: [LayerInfo] = []
    var activeLayer = 0
    /// Dormant history branches, when the experimental feature is on.
    var historyBranches: [HistoryBranchInfo] = []
    /// Whether an undo sequence has an abandoned path the next edit would keep as a branch.
    var branchPending = false
    var canUndo = false
    var canRedo = false
    /// A short confirmation in the status bar, cleared after a moment.
    var status: StatusMessage?
    /// True while no layer has paint, so the shell can invite a first line.
    var isBlank = true
    var thumbnails: [CGImage?] = []
    var frameCount = 1
    var activeFrame = 0
    var frameImages: [CGImage?] = []
    var onionImage: CGImage?
    var onionEnabled = false {
        didSet { refreshOnion() }
    }
    var onionBefore = 1 {
        didSet { refreshOnion() }
    }
    var onionAfter = 0 {
        didSet { refreshOnion() }
    }
    var isPlaying = false
    var fps = 12 {
        didSet {
            if handle != nil {
                dk_set_fps(handle, UInt32(min(max(fps, 1), 60)))
            }
        }
    }
    /// Hold of the active frame in milliseconds.
    var frameDuration = 83 {
        didSet {
            if handle != nil, Int(dk_frame_duration(handle)) != frameDuration {
                dk_set_frame_duration(handle, UInt32(min(max(frameDuration, 10), 10_000)))
            }
        }
    }
    var loopEnabled = true {
        didSet {
            if handle != nil, (dk_loop_enabled(handle) != 0) != loopEnabled {
                dk_set_loop_enabled(handle, loopEnabled ? 1 : 0)
            }
        }
    }
    var loopStart = 0 {
        didSet {
            if handle != nil, Int(dk_loop_start(handle)) != loopStart {
                dk_set_loop_start(handle, UInt32(max(loopStart, 0)))
            }
        }
    }
    /// One past the last looping frame.
    var loopEnd = 1 {
        didSet {
            if handle != nil, Int(dk_loop_end(handle)) != loopEnd {
                dk_set_loop_end(handle, UInt32(max(loopEnd, 1)))
            }
        }
    }
    @ObservationIgnored var playbackTask: Task<Void, Never>?
    /// The whole painting, for the navigator. Refreshed with the layer thumbnails.
    var overview: CGImage?
    /// What the GPU texture still needs from the last composite; consumed by the surface.
    /// This is render bookkeeping, not view state: observing it made `takeSurfaceDirty` inside
    /// `updateNSView` invalidate the same view, and while the welcome screen occluded the canvas
    /// the failed present marked it full again - a SwiftUI update loop.
    @ObservationIgnored private(set) var surfaceDirty: SurfaceRect?
    /// Bumped whenever the canvas pixels change. `@Observable` scopes invalidation to views that
    /// read a property, and an NSView is not one of them; the representable takes this as an input
    /// so SwiftUI calls `updateNSView` and the view redraws after undo or a layer change.
    var revision: UInt64 = 0

    @ObservationIgnored private let core = CanvasHandle()
    var handle: UnsafeMutableRawPointer? { core.pointer }
    var pixels: PixelBuffer
    @ObservationIgnored var linearPixels: LinearBuffer?
    @ObservationIgnored private var started = Date()
    @ObservationIgnored var statusTask: Task<Void, Never>?
    private(set) var width: Int
    private(set) var height: Int
    /// The file this painting came from or was saved to, for recents and reopening.
    var documentURL: URL?
    private(set) var colorSpace: PaintColorSpace = .sRGB
    private(set) var hdr = false
    /// True after an edit and false after a successful save or open. Drives autosave and the
    /// unsaved marker in the status strip.
    var isDirty = false
    var lastAutosave: Date?
    @ObservationIgnored var mirrorEnabled = false
    @ObservationIgnored var mirrorAngle: Double = 90
    @ObservationIgnored var mirrorBoth = false
    @ObservationIgnored var brushKind: BrushKind = .softRound
    @ObservationIgnored var thumbnailTask: Task<Void, Never>?
    /// Counts edits so a background save can tell whether the canvas moved while it wrote.
    @ObservationIgnored var editCount = 0
    @ObservationIgnored var autosaving = false
    /// Bumped when the document is replaced, so a finishing background save does not address the
    /// document that took its place.
    @ObservationIgnored var documentEpoch = 0
    /// Bumped when a recovery copy is discarded, so an autosave that was already running does not
    /// bring it back afterwards.
    @ObservationIgnored var autosaveEpoch = 0
    /// After a discard, do not write the same recovery copy again until the person edits.
    @ObservationIgnored var autosaveBlockedUntilEdit = true
    @ObservationIgnored var timelapseExporting = false
    @ObservationIgnored var gpuLayersDirty = true
    @ObservationIgnored var gpuSpecsDirty = true
    /// Set by the canvas view when the SDR surface can composite resident layers.
    @ObservationIgnored var gpuCanvas = false
    @ObservationIgnored var fillSnap = true
    @ObservationIgnored var strokeDedupe = true
    @ObservationIgnored var prediction = true
    @ObservationIgnored var quickShape = true
    /// The in-flight fill reveal; it is not undo or tape until the task finishes.
    @ObservationIgnored var fillTask: Task<Void, Never>?
    var autoShapeEnabled = false
    /// Transient rectangular selection in document pixels. It is not saved in the project.
    var selection: SurfaceRect?
    var stride: Int { width * 4 }

    func edited() {
        stopPlayback()
        autosaveBlockedUntilEdit = false
        if !isDirty {
            isDirty = true
        }
        editCount &+= 1
        refreshHistoryBranches()
    }

    /// Adobe RGB has no eight-bit drawable, so it presents through the linear path like HDR.
    var needsLinear: Bool { hdr || colorSpace == .adobeRgb }

    init(width: Int = 1024, height: Int = 1024, colorSpace: PaintColorSpace = .sRGB,
         hdr: Bool = false) {
        let clampedWidth = min(max(width, 16), 8192)
        let clampedHeight = min(max(height, 16), 8192)
        self.width = clampedWidth
        self.height = clampedHeight
        self.colorSpace = colorSpace
        self.hdr = hdr
        self.pixels = PixelBuffer(count: clampedWidth * clampedHeight * 4)
        self.linearPixels = (hdr || colorSpace == .adobeRgb)
            ? LinearBuffer(count: clampedWidth * clampedHeight * 4) : nil
        core.pointer = dk_editor_new(UInt32(clampedWidth), UInt32(clampedHeight))
        dk_set_color_space(core.pointer, Int32(colorSpace.rawValue))
        dk_set_dynamic_range(core.pointer, hdr ? 1 : 0)
        dk_set_fill_snap(core.pointer, fillSnap ? 1 : 0)
        dk_set_stroke_dedupe(core.pointer, strokeDedupe ? 1 : 0)
        dk_set_prediction(core.pointer, prediction ? 1 : 0)
        dk_set_quick_shape(core.pointer, quickShape ? 1 : 0)
        dk_set_brush(core.pointer, Int32(brushKind.rawValue))
        refreshBrush()
        refreshBrushPreviews()
        refreshBrushTexture()
        syncLayers()
        refresh()
        refreshThumbnails()
    }

    /// Start a new painting at an explicit size. The old handle and buffer are replaced; tools and
    /// window state stay where the person left them.
    func reset(width: Int, height: Int, colorSpace: PaintColorSpace = .sRGB, hdr: Bool = false) {
        stopPlayback()
        fillTask?.cancel()
        fillTask = nil
        let clampedWidth = min(max(width, 16), 8192)
        let clampedHeight = min(max(height, 16), 8192)
        self.colorSpace = colorSpace
        self.hdr = hdr
        dk_editor_free(handle)
        documentEpoch &+= 1
        self.width = clampedWidth
        self.height = clampedHeight
        core.pointer = dk_editor_new(UInt32(clampedWidth), UInt32(clampedHeight))
        dk_set_color_space(core.pointer, Int32(colorSpace.rawValue))
        dk_set_dynamic_range(core.pointer, hdr ? 1 : 0)
        dk_set_fill_snap(core.pointer, fillSnap ? 1 : 0)
        dk_set_stroke_dedupe(core.pointer, strokeDedupe ? 1 : 0)
        dk_set_prediction(core.pointer, prediction ? 1 : 0)
        dk_set_quick_shape(core.pointer, quickShape ? 1 : 0)
        dk_set_brush(core.pointer, Int32(brushKind.rawValue))
        refreshBrush()
        refreshBrushPreviews()
        refreshBrushTexture()
        pixels = PixelBuffer(count: clampedWidth * clampedHeight * 4)
        linearPixels = needsLinear ? LinearBuffer(count: clampedWidth * clampedHeight * 4) : nil
        documentURL = nil
        image = nil
        recordingMs = 0
        previewMs = nil
        status = nil
        isBlank = true
        isDirty = false
        canUndo = false
        canRedo = false
        started = Date()
        dk_record_enabled(handle, 1)
        applyMirror()
        dk_set_brush(handle, Int32(brushKind.rawValue))
        refreshBrush()
        refreshBrushPreviews()
        refreshBrushTexture()
        dk_set_fill_snap(handle, fillSnap ? 1 : 0)
        dk_set_stroke_dedupe(handle, strokeDedupe ? 1 : 0)
        dk_set_prediction(handle, prediction ? 1 : 0)
        dk_set_quick_shape(handle, quickShape ? 1 : 0)
        syncLayers()
        markGPURebuild()
        refresh()
        refreshThumbnails()
        refreshFrames()
        discardAutosave()
        revision &+= 1
    }

    /// Open a project, Krita/Photoshop document or image at the file's own size. A recovered copy
    /// keeps its original source name and starts dirty, so the next save asks for a destination.
    @discardableResult
    func openDocument(_ url: URL, recovered: Bool = false) -> Bool {
        stopPlayback()
        fillTask?.cancel()
        fillTask = nil
        guard let opened = url.path.withCString({ dk_open_file($0) }) else {
            NSSound.beep()
            flash("Could not open \(url.lastPathComponent)", error: true)
            return false
        }
        dk_editor_free(handle)
        documentEpoch &+= 1
        core.pointer = opened
        width = Int(dk_width(handle))
        height = Int(dk_height(handle))
        colorSpace = PaintColorSpace(rawValue: Int(dk_color_space(handle))) ?? .sRGB
        hdr = dk_dynamic_range(handle) != 0
        pixels = PixelBuffer(count: width * height * 4)
        linearPixels = needsLinear ? LinearBuffer(count: width * height * 4) : nil
        documentURL = recovered ? nil : url
        recordingMs = 0
        previewMs = nil
        status = nil
        started = Date()
        dk_record_enabled(handle, 1)
        applyMirror()
        dk_set_brush(handle, Int32(brushKind.rawValue))
        refreshBrush()
        refreshBrushPreviews()
        refreshBrushTexture()
        dk_set_fill_snap(handle, fillSnap ? 1 : 0)
        dk_set_stroke_dedupe(handle, strokeDedupe ? 1 : 0)
        dk_set_prediction(handle, prediction ? 1 : 0)
        dk_set_quick_shape(handle, quickShape ? 1 : 0)
        syncLayers()
        markGPURebuild()
        redrawAll()
        refreshThumbnails()
        refreshFrames()
        recordingMs = dk_recording_ms(handle)
        isDirty = recovered
        if recovered {
            discardAutosave()
        } else {
            discardAutosave()
            RecentDocuments.add(url)
            cacheThumbnail(for: url)
        }
        flash(recovered ? "Recovered" : "Opened \(url.lastPathComponent)")
        return true
    }

    /// Adopt the geometry of an editor that replaced the document in place.
    func adoptImportedGeometry() {
        width = Int(dk_width(handle))
        height = Int(dk_height(handle))
        colorSpace = PaintColorSpace(rawValue: Int(dk_color_space(handle))) ?? .sRGB
        hdr = dk_dynamic_range(handle) != 0
    }

    /// Milliseconds since this painting began. The tape is the document, so every stroke carries a
    /// timestamp even when nobody asked for a replay; otherwise all the events would sit at zero
    /// and a timelapse would have nothing to order.
    var now: UInt64 {
        UInt64(max(0, Date().timeIntervalSince(started)) * 1000)
    }

    // MARK: Recording

    // MARK: Image

    func refresh(notify: Bool = true) {
        refreshHistoryBranches()
        if previewMs == nil && gpuEligible {
            // The canvas composites the resident layer textures; the CPU pixels are only rebuilt
            // when an export or a thumbnail asks for them.
            markSurfaceFull()
        } else if let ms = previewMs {
            if tapeReplay == nil && !tapeReplayUnavailable, let handle {
                tapeReplay = TapeReplay(handle: handle, width: width, height: height)
                tapeReplayUnavailable = tapeReplay == nil
            }
            if tapeReplay?.frame(at: ms, into: pixels.pointer) != true {
                _ = dk_replay_composite(handle, ms, pixels.pointer)
            }
            markSurfaceFull()
        } else if let rect = takeDirty() {
            composite(rect)
            if hdr { compositeLinear(rect) }
            markSurfaceDirty(SurfaceRect(x: rect.x, y: rect.y,
                                         width: rect.width, height: rect.height))
        } else {
            _ = dk_composite(handle, pixels.pointer)
            markSurfaceFull()
        }
        if !gpuEligible {
            image = makeImage()
        }
        canUndo = dk_can_undo(handle) != 0
        canRedo = dk_can_redo(handle) != 0
        // The tape records whether or not the replay bar is open, so the duration follows the
        // tape, not the record button.
        recordingMs = dk_recording_ms(handle)
        if notify {
            revision &+= 1
        }
    }

    /// The texture keeps its previous contents; the surface only needs what changed.
    func takeSurfaceDirty() -> SurfaceRect? {
        defer { surfaceDirty = nil }
        return surfaceDirty
    }

    func markSurfaceFull() {
        materializeCanvas()
        surfaceDirty = SurfaceRect(x: 0, y: 0, width: width, height: height)
    }

    private func markSurfaceDirty(_ rect: SurfaceRect) {
        guard let current = surfaceDirty else {
            surfaceDirty = rect
            return
        }
        let x = min(current.x, rect.x)
        let y = min(current.y, rect.y)
        let right = max(current.x + current.width, rect.x + rect.width)
        let bottom = max(current.y + current.height, rect.y + rect.height)
        surfaceDirty = SurfaceRect(x: x, y: y, width: right - x, height: bottom - y)
    }

    /// Re-composite the whole document. Visibility and opacity change every pixel without marking
    /// any layer dirty, and the core has no opinion about what the shell should redraw.
    /// Layer opacity, visibility and blend change the GPU composite, not the layer pixels.
    func afterLayerStateChange() {
        gpuSpecsDirty = true
        canUndo = dk_can_undo(handle) != 0
        canRedo = dk_can_redo(handle) != 0
        if !gpuEligible {
            redrawAll()
        }
        revision &+= 1
    }

    func redrawAll() {
        canUndo = dk_can_undo(handle) != 0
        canRedo = dk_can_redo(handle) != 0
        recordingMs = dk_recording_ms(handle)
        if gpuEligible {
            markGPURebuild()
            markSurfaceFull()
            revision &+= 1
            return
        }
        _ = dk_composite(handle, pixels.pointer)
        if let linear = linearPixels {
            _ = dk_composite_linear(handle, linear.pointer)
        }
        markSurfaceFull()
        image = makeImage()
        revision &+= 1
    }

    private func takeDirty() -> (x: Int, y: Int, width: Int, height: Int)? {
        var rect = [UInt32](repeating: 0, count: 4)
        let found = rect.withUnsafeMutableBufferPointer {
            dk_take_dirty(handle, $0.baseAddress)
        }
        guard found != 0, rect[2] > 0, rect[3] > 0 else { return nil }
        return (Int(rect[0]), Int(rect[1]), Int(rect[2]), Int(rect[3]))
    }

    private func composite(_ rect: (x: Int, y: Int, width: Int, height: Int)) {
        var tile = [UInt8](repeating: 0, count: rect.width * rect.height * 4)
        let drawn = tile.withUnsafeMutableBytes { bytes in
            dk_composite_rect(handle, UInt32(rect.x), UInt32(rect.y), UInt32(rect.width),
                              UInt32(rect.height), bytes.bindMemory(to: UInt8.self).baseAddress)
        }
        guard drawn != 0 else { return }
        tile.withUnsafeBufferPointer { source in
            guard let base = source.baseAddress else { return }
            for row in 0..<rect.height {
                let destination = pixels.pointer + (rect.y + row) * stride + rect.x * 4
                destination.update(from: base + row * rect.width * 4, count: rect.width * 4)
            }
        }
    }

    private func compositeLinear(_ rect: (x: Int, y: Int, width: Int, height: Int)) {
        guard let linear = linearPixels else { return }
        var tile = [Float](repeating: 0, count: rect.width * rect.height * 4)
        let drawn = tile.withUnsafeMutableBufferPointer {
            dk_composite_linear_rect(handle, UInt32(rect.x), UInt32(rect.y), UInt32(rect.width),
                                     UInt32(rect.height), $0.baseAddress)
        }
        guard drawn != 0 else { return }
        tile.withUnsafeBufferPointer { source in
            guard let base = source.baseAddress else { return }
            for row in 0..<rect.height {
                let destination = linear.pointer + (rect.y + row) * stride + rect.x * 4
                destination.update(from: base + row * rect.width * 4, count: rect.width * 4)
            }
        }
    }




}


// The AppKit bridge for the shared colour-space type.
extension PaintColorSpace {
    var nsColorSpace: NSColorSpace {
        switch self {
        case .sRGB: .sRGB
        case .displayP3: .displayP3
        case .adobeRgb: .adobeRGB1998
        }
    }
}
