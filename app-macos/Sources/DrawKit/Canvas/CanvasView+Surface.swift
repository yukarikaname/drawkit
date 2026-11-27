// The wgpu surface behind the canvas and the upload/present path.

import SwiftUI
import AppKit
import QuartzCore

extension CanvasView {
    private func surfaceLog(_ message: String) {
        FileHandle.standardError.write(Data(("DrawKit canvas: \(message)\n").utf8))
    }

    /// Let the model know whether the current surface can composite resident layers.
    func refreshGPUAvailability() {
        model?.setGPUCanvasAvailable(surface?.supportsLayers ?? false)
    }

    func createSurface() {
        guard surface == nil, let metal = layer as? CAMetalLayer,
              bounds.width > 0, bounds.height > 0 else { return }
        let size = convertToBacking(bounds).size
        guard size.width > 0, size.height > 0 else { return }
        metal.contentsScale = window?.backingScaleFactor ?? 2
        metal.drawableSize = size
        surface = PaintSurface(layer: metal, width: Int(size.width), height: Int(size.height),
                               extended: extended, colorSpace: colorSpace)
        surfaceExtended = surface?.extended ?? false
        model?.paintSurface = surface
        surface?.setExposure(Float(exposure))
        surface?.setLayerBudget(bytes: 1536 << 20)
        if surface != nil && extended && !surfaceExtended {
            // The machine has no extended-range drawable: the eight-bit fallback is tagged in the
            // document space, which is close enough to stay readable.
            surfaceLog("extended range unavailable; falling back to eight-bit")
        }
        model?.setGPUCanvasAvailable(surface?.supportsLayers ?? false)
        gpuLayerCount = 0
        // The palette previews now come from the resident stack, which did not exist when the
        // document opened; fill the rows once it does.
        if surface != nil {
            model?.scheduleThumbnails()
        }
        surfaceLog(surface == nil ? "canvas surface unavailable"
                                  : "canvas surface \(Int(size.width))×\(Int(size.height))"
                                    + (surfaceExtended ? " extended range" : ""))
    }

    func setExposure(_ value: Double) {
        exposure = value
        surface?.setExposure(Float(value))
    }

    /// Switch the surface to or from the extended linear path. The drawable format cannot change
    /// in place, so the surface is recreated; the next present uploads the whole canvas.
    func setExtended(_ value: Bool) {
        guard value != extended else { return }
        extended = value
        recreateSurface()
    }

    func recreateSurface() {
        surface = nil
        surfaceExtended = false
        createSurface()
        presentSurface(force: true, upload: true)
    }

    /// The resident composite can miss a frame while the window is occluded or a drawable is
    /// still being built. Retry the GPU path; the CPU cache is not kept in step with a GPU stroke,
    /// so there is nothing stale to fall back to.
    private func retryGPULayers() {
        gpuFailures += 1
        if gpuFailures > 3 {
            surfaceLog("GPU layer present failed")
            return
        }
        surfaceLog(gpuFailures == 1 ? "GPU layer present failed; retrying"
                                    : "GPU layer present still failing; retrying")
        gpuRetry?.cancel()
        let work = DispatchWorkItem { [weak self] in
            guard let self, self.surface != nil else { return }
            self.presentSurface(force: true, upload: false)
        }
        gpuRetry = work
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.2 * Double(gpuFailures),
                                      execute: work)
    }

    /// Upload the painting if its revision changed. Pan and zoom force the call because the paper
    /// rectangle is a uniform even when the pixels did not move.
    ///
    /// The resident layers are the document: a present failure retries the GPU path rather than
    /// repainting a CPU composite the strokes never reached. The one frame that does not live in
    /// the stack is the timeline's, which arrives as pixels.
    func presentSurface(force: Bool = false, upload: Bool = true) {
        let probeStart = presentProbe == nil ? 0 : CFAbsoluteTimeGetCurrent()
        defer {
            if let probe = presentProbe {
                probe((CFAbsoluteTimeGetCurrent() - probeStart) * 1000)
            }
        }
        guard let model, bounds.width > 0, bounds.height > 0 else {
            overlay.needsDisplay = true
            return
        }
        guard let surface else {
            overlay.needsDisplay = true
            return
        }
        let scale = window?.backingScaleFactor ?? 2
        let rect = imageRect
        guard rect.width > 0, rect.height > 0 else { return }
        let top = (bounds.height - rect.maxY) * scale
        let paper = CGRect(x: rect.minX * scale, y: top,
                           width: rect.width * scale, height: rect.height * scale)

        if model.previewMs != nil {
            lastPresentCode = model.presentPreview(surface: surface, paper: paper) ? 1 : 0
            if lastPresentCode == 0 {
                surfaceLog("preview present failed")
            }
            CATransaction.flush()
            overlay.needsDisplay = true
            return
        }

        var uploaded = true
        let count = model.layers.count
        // A layout pass can ask for a present before the first upload. The layer stack is
        // still empty at that point, so rebuild whenever the resident stack does not match.
        // The stack's own count is the truth: an incremental insert or remove leaves it matching
        // the document, while a rebuild flag or a real mismatch still resets.
        let rebuilt = model.gpuNeedsRebuild || surface.residentLayerCount != count
        if upload || rebuilt {
            if rebuilt {
                surface.resetLayers(count: count, width: model.width, height: model.height)
                for index in 0..<count where uploaded {
                    // An empty layer has no pixels to upload; its host is already in the stack.
                    if model.layerIsClear(index: index) { continue }
                    uploaded = model.uploadLayerRect(
                        surface, index: index, x: 0, y: 0,
                        width: model.width, height: model.height)
                }
                gpuLayerCount = count
            } else {
                for index in 0..<count where uploaded {
                    guard let dirty = model.takeGPUDirty(index: index) else { continue }
                    uploaded = model.uploadLayerRect(
                        surface, index: index, x: dirty.x, y: dirty.y,
                        width: dirty.width, height: dirty.height)
                }
            }
            if uploaded {
                // The budget can evict a layer while the others upload; put it back before
                // the composite or that layer silently disappears from the frame.
                for index in 0..<count where surface.layerMissing(index) {
                    uploaded = model.uploadLayerRect(
                        surface, index: index, x: 0, y: 0,
                        width: model.width, height: model.height)
                    if !uploaded { break }
                }
            }
            // A backup-less stack reports the tiles its budget evicted. The core still has
            // them, so put them back before this frame is composed.
            if uploaded {
                for rect in surface.missingRects(limit: 64) {
                    uploaded = model.uploadLayerRect(
                        surface, index: rect.layer, x: rect.x, y: rect.y,
                        width: rect.width, height: rect.height)
                    if !uploaded { break }
                }
            }
            if uploaded {
                // A spec change marks every tile dirty, so only touch the layer parameters
                // when the layer tree actually changed; a stroke upload must stay local.
                if model.gpuSpecsNeedUpdate {
                    for index in 0..<count {
                        guard let state = model.gpuLayerState(index: index) else { continue }
                        surface.setLayer(index: index, opacity: state.opacity,
                                         blend: Int(state.blend), visible: state.visible,
                                         fill: state.fill)
                        surface.setLayerParent(index: index, parent: state.parent)
                        surface.setLayerClip(index: index,
                                             base: Int32(model.gpuLayerClip(index: index)))
                    }
                    model.gpuSpecsUpdated()
                }
                model.gpuUploadsComplete()
                if rebuilt {
                    // The palette previews come from the resident stack, which only now holds
                    // the document that asked for the rebuild.
                    model.scheduleThumbnails()
                }
            } else {
                surfaceLog("GPU layer upload failed; retrying")
                model.markGPURebuild()
                retryGPULayers()
                return
            }
        }
        let traceStart = presentTrace ? CFAbsoluteTimeGetCurrent() : 0
        let presented = surface.presentLayers(paper: paper)
        let composed = presentTrace ? CFAbsoluteTimeGetCurrent() : 0
        lastPresentCode = presented
        presentCount += 1
        // Flushing commits to the window server synchronously, and that round trip measured 13-28
        // ms: on the stroke path it is exactly the stall that makes a pen feel late. Only a frame
        // that has to be on screen before the caller returns — a fill, an undo, a layer change —
        // pays it. A stroke frame is committed by the runloop's own transaction microseconds
        // later. Inside updateNSView nothing may flush at all: the hosting view is mid-layout and
        // flushing there lays it out reentrantly.
        if force && !inUpdatePass {
            CATransaction.flush()
        }
        if presentTrace {
            let now = CFAbsoluteTimeGetCurrent()
            FileHandle.standardError.write(Data(String(
                format: "DRAWKIT_PRESENT compose %.2f ms, flush %.2f ms\n",
                (composed - traceStart) * 1000, (now - composed) * 1000).utf8))
        }
        if presented == 1 {
            gpuFailures = 0
            gpuRetry?.cancel()
            gpuRetry = nil
            reportedSurface = true
        } else if presented == 2 {
            // The layers composed; only the window was not on screen. Keep the frame and try
            // again next time rather than drawing anything else.
            reportedSurface = true
        } else {
            surfaceLog("GPU layer present failed; retrying")
            retryGPULayers()
        }
        overlay.needsDisplay = true
    }


/// The display pace. ProMotion reports 120; a 60 Hz panel reports 60.
var presentInterval: TimeInterval {
    if let hz = presentHzOverride { return 1.0 / hz }
    let hz = Double(window?.screen?.maximumFramesPerSecond ?? 60)
    return 1.0 / min(max(hz, 30), 240)
}

/// Show a stroke frame, at most once per display refresh.
///
/// A pen sample arrives every millisecond or two, but presenting blocks until the window server
/// frees a drawable: `get_current_texture` was measured at 3-11 ms inside a stroke. Presenting every
/// sample therefore queues the pen behind the swapchain, and the line falls further behind the
/// longer the stroke runs. Painting a sample costs 0.05 ms and showing it costs a frame, so paint
/// every sample and show the newest state once per refresh.
func presentStrokeFrame() {
    let now = CFAbsoluteTimeGetCurrent()
    let interval = presentInterval * 0.9
    if now - lastStrokePresent < interval {
        // The refresh has not come round yet. Keep the newest painting for when it does, so the
        // last segment is shown even if the pen stops right here.
        deferStrokePresent(after: interval - (now - lastStrokePresent))
        return
    }
    // This frame is being shown now; a deferred one would only present the same pixels again.
    deferredPresent?.cancel()
    deferredPresent = nil
    presentSurface()
    lastStrokePresent = CFAbsoluteTimeGetCurrent()
}

/// Present whatever was painted when the deferred refresh arrives.
private func deferStrokePresent(after delay: TimeInterval) {
    guard deferredPresent == nil else { return }
    let work = DispatchWorkItem { [weak self] in
        guard let self else { return }
        deferredPresent = nil
        lastStrokePresent = CFAbsoluteTimeGetCurrent()
        presentSurface()
    }
    deferredPresent = work
    DispatchQueue.main.asyncAfter(deadline: .now() + max(0, delay), execute: work)
}

/// Show the finished stroke at once instead of waiting for its refresh slot.
func finishStrokePresent(upload: Bool = false) {
    deferredPresent?.cancel()
    deferredPresent = nil
    lastStrokePresent = CFAbsoluteTimeGetCurrent()
    presentSurface(force: true, upload: upload)
}
}
