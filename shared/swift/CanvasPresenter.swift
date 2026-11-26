// The upload and present path shared by the canvas shells. The presenter knows when the resident
// layer stack is stale; the shell knows how the document stores its layers.

import CoreGraphics

/// What the presenter needs from a document. Everything is read on the main actor, where the
/// shell mutates the layers and presents.
@MainActor
protocol CanvasPresenterSource: AnyObject {
    var layerCount: Int { get }
    var canvasWidth: Int { get }
    var canvasHeight: Int { get }
    /// A whole-canvas composite replaced the core cache, so every layer has to be uploaded.
    var needsRebuild: Bool { get }
    /// Clear the rebuild request after the stack reached the surface.
    func rebuildCompleted()
    /// Upload one rectangle of one layer in whatever form the surface composes.
    func uploadLayer(_ surface: PaintSurface, index: Int, x: Int, y: Int,
                     width: Int, height: Int) -> Bool
    /// The rectangle this layer still has to upload, consumed on read.
    func takeDirtyRect(index: Int) -> (x: Int, y: Int, width: Int, height: Int)?
    /// Push the layer tree's parameters to the surface.
    func updateLayerSpecs(_ surface: PaintSurface)
}

/// Keeps a surface's resident layers in step with a document and presents them.
@MainActor
final class CanvasPresenter {
    let surface: PaintSurface
    weak var source: (any CanvasPresenterSource)?
    /// Called after every present attempt, so the shell can commit the frame and redraw chrome.
    var afterPresent: ((Int) -> Void)?

    /// The layer count the resident stack was built with, or nil when it has to be rebuilt.
    private(set) var residentLayers: Int?
    private var failures = 0
    private var retry: Task<Void, Never>?
    private var paper = CGRect.zero

    init(surface: PaintSurface, source: any CanvasPresenterSource) {
        self.surface = surface
        self.source = source
    }

    /// Forget the resident stack, for a new drawable or a format change.
    func reset() {
        residentLayers = nil
        failures = 0
        retry?.cancel()
        retry = nil
    }

    /// Upload what the document still needs and composite the resident layers.
    /// The return value is `presentLayers`': 1 drawn, 2 composed but not presentable, 0 failed.
    @discardableResult
    func present(paper: CGRect) -> Int {
        guard let source else { return 0 }
        self.paper = paper
        let count = source.layerCount
        let rebuild = source.needsRebuild || residentLayers != count
        let uploaded = rebuild ? uploadAll(source, count: count)
                               : uploadDirty(source, count: count)
        // The budget can evict a layer while the others upload; put it back before the
        // composite or that layer silently disappears from the frame.
        guard uploaded, restoreEvicted(source, count: count) else {
            // A consumed rectangle cannot be requested again, so restart from a full upload.
            residentLayers = nil
            scheduleRetry()
            afterPresent?(0)
            return 0
        }
        if rebuild {
            source.rebuildCompleted()
        }
        source.updateLayerSpecs(surface)
        let presented = surface.presentLayers(paper: paper)
        if presented == 1 {
            failures = 0
            retry?.cancel()
            retry = nil
        } else if presented == 0 {
            scheduleRetry()
        }
        afterPresent?(presented)
        return presented
    }

    private func uploadAll(_ source: any CanvasPresenterSource, count: Int) -> Bool {
        surface.resetLayers(count: count, width: source.canvasWidth, height: source.canvasHeight)
        residentLayers = count
        for index in 0..<count {
            guard source.uploadLayer(surface, index: index, x: 0, y: 0,
                                     width: source.canvasWidth,
                                     height: source.canvasHeight) else {
                return false
            }
            // The full upload already carried these pixels; do not upload them a second time.
            _ = source.takeDirtyRect(index: index)
        }
        return true
    }

    private func uploadDirty(_ source: any CanvasPresenterSource, count: Int) -> Bool {
        for index in 0..<count {
            guard let rect = source.takeDirtyRect(index: index) else { continue }
            guard source.uploadLayer(surface, index: index, x: rect.x, y: rect.y,
                                     width: rect.width, height: rect.height) else {
                return false
            }
        }
        return true
    }

    private func restoreEvicted(_ source: any CanvasPresenterSource, count: Int) -> Bool {
        for index in 0..<count where surface.layerMissing(index) {
            guard source.uploadLayer(surface, index: index, x: 0, y: 0,
                                     width: source.canvasWidth,
                                     height: source.canvasHeight) else {
                return false
            }
        }
        return true
    }

    /// Retry the GPU path. The CPU composite is not kept in step with a GPU stroke, so there is
    /// no stale frame to fall back to.
    private func scheduleRetry() {
        failures += 1
        guard failures <= 3 else { return }
        retry?.cancel()
        let delay = 200 * failures
        retry = Task { @MainActor [weak self] in
            try? await Task.sleep(for: .milliseconds(delay))
            guard !Task.isCancelled, let self, self.source != nil else { return }
            _ = self.present(paper: self.paper)
        }
    }
}
