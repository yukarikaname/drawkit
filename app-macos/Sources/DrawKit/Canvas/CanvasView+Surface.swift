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
        surface?.setExposure(Float(exposure))
        if surface != nil && extended && !surfaceExtended {
            // The machine has no extended-range drawable: the eight-bit fallback is tagged in the
            // document space, which is close enough to stay readable.
            surfaceLog("extended range unavailable; falling back to eight-bit")
        }
        model?.markSurfaceFull()
        model?.setGPUCanvasAvailable(surface?.supportsLayers ?? false)
        gpuLayerCount = 0
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

    /// A present can fail once on a new drawable or an empty resident stack. Falling back to the
    /// CPU is right for that frame, but disabling the GPU for the rest of the session is not: retry
    /// with a short backoff and only stay on the CPU after repeated failures.
    private func retryGPULayers() {
        gpuFailures += 1
        if gpuFailures > 3 {
            surfaceLog("GPU layer present failed; using CPU composite")
            return
        }
        surfaceLog(gpuFailures == 1 ? "GPU layer present failed; retrying"
                                    : "GPU layer present still failing; retrying")
        gpuRetry?.cancel()
        let work = DispatchWorkItem { [weak self] in
            guard let self, let model = self.model, let surface = self.surface,
                  surface.supportsLayers, !self.surfaceExtended else { return }
            model.setGPUCanvasAvailable(true)
            model.markGPURebuild()
            model.markSurfaceFull()
            self.presentSurface(force: true, upload: true)
        }
        gpuRetry = work
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.2 * Double(gpuFailures),
                                      execute: work)
    }

    /// Upload the painting if its revision changed. Pan and zoom force the call because the paper
    /// rectangle is a uniform even when the pixels did not move.
    func presentSurface(force: Bool = false, upload: Bool = true, allowRecovery: Bool = true) {
        guard let surface, let model, bounds.width > 0, bounds.height > 0 else {
            overlay.needsDisplay = true
            return
        }
        let scale = window?.backingScaleFactor ?? 2
        let rect = imageRect
        guard rect.width > 0, rect.height > 0 else { return }
        let top = (bounds.height - rect.maxY) * scale
        let paper = CGRect(x: rect.minX * scale, y: top,
                           width: rect.width * scale, height: rect.height * scale)

        let gpu = !surfaceExtended && model.previewMs == nil
            && model.gpuEligible && surface.supportsLayers
        if gpu {
            var uploaded = true
            let count = model.layers.count
            // A layout pass can ask for a present before the first upload. The layer stack is
            // still empty at that point, so present_layers would report a failure and drop the
            // whole canvas to the CPU. Rebuild whenever the resident stack does not match.
            if upload || model.gpuNeedsRebuild || gpuLayerCount != count {
                if model.gpuNeedsRebuild || gpuLayerCount != count {
                    surface.resetLayers(count: count, width: model.width, height: model.height)
                    for index in 0..<count where uploaded {
                        uploaded = model.withLayerPixels(
                            index: index, x: 0, y: 0,
                            width: model.width, height: model.height
                        ) { pixels in
                            surface.uploadLayer(index: index, x: 0, y: 0,
                                                width: model.width, height: model.height,
                                                pixels: pixels)
                        }
                    }
                    gpuLayerCount = count
                } else {
                    for index in 0..<count where uploaded {
                        guard let dirty = model.takeGPUDirty(index: index) else { continue }
                        uploaded = model.withLayerPixels(
                            index: index, x: dirty.x, y: dirty.y,
                            width: dirty.width, height: dirty.height
                        ) { pixels in
                            surface.uploadLayer(index: index, x: dirty.x, y: dirty.y,
                                                width: dirty.width, height: dirty.height,
                                                pixels: pixels)
                        }
                    }
                }
                if uploaded {
                    for index in 0..<count {
                        guard let state = model.gpuLayerState(index: index) else { continue }
                        surface.setLayer(index: index, opacity: state.opacity,
                                         blend: Int(state.blend), visible: state.visible,
                                         fill: state.fill)
                    }
                    model.gpuUploadsComplete()
                } else {
                    model.setGPUCanvasAvailable(false)
                    model.markGPURebuild()
                    model.markSurfaceFull()
                    model.refreshCPUImage()
                    surfaceLog("GPU layer upload failed; using CPU composite")
                }
            }
            let presented = model.gpuEligible ? surface.presentLayers(paper: paper) : 0
            if presented == 1 {
                gpuFailures = 0
                gpuRetry?.cancel()
                gpuRetry = nil
                reportedSurface = true
                overlay.needsDisplay = true
                return
            }
            if presented == 2 {
                // The layers composed; only the window was not on screen. Keep the GPU path and
                // try again next frame instead of dropping to a stale CPU buffer.
                reportedSurface = true
                overlay.needsDisplay = true
                return
            }
            if model.gpuEligible {
                model.setGPUCanvasAvailable(false)
                model.markGPURebuild()
                model.markSurfaceFull()
                model.refreshCPUImage()
                retryGPULayers()
            }
        }

        let dirty = upload ? model.takeSurfaceDirty() : nil
        if dirty == nil && !force {
            overlay.needsDisplay = true
            return
        }
        let dirtyRect = dirty.map {
            CGRect(x: $0.x, y: $0.y, width: $0.width, height: $0.height)
        }
        model.withSurfacePixels(hdr: surfaceExtended) { pixels, width, height, linear in
            let presented = surface.present(pixels: pixels, width: width, height: height,
                                            paper: paper, dirty: dirtyRect,
                                            linear: linear)
            if !presented {
                model.markSurfaceFull()
                if allowRecovery {
                    // A lost device or a dead swapchain leaves the old surface unusable. Build a
                    // new one and upload the whole canvas; the retry never recurses further.
                    self.surface = nil
                    surfaceExtended = false
                    self.createSurface()
                    if self.surface != nil {
                        self.presentSurface(force: true, upload: true, allowRecovery: false)
                    } else {
                        surfaceLog("canvas surface present failed; no replacement")
                    }
                } else {
                    surfaceLog("canvas surface present failed")
                }
            } else if !reportedSurface {
                reportedSurface = true
                surfaceLog("canvas surface presenting \(width)×\(height)")
            }
        }
        overlay.needsDisplay = true
    }
}
