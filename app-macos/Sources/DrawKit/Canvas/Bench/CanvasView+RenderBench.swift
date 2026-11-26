// A launch-time render benchmark for the resolution-independent path. It paints one stroke with
// each thickness policy and replays both at half size, then measures the painted thickness in the
// output. Only runs when DRAWKIT_BENCH_RENDER is set.

import AppKit
import Foundation

extension CanvasView {
    static var renderBenchmarkRequested: Bool {
        ProcessInfo.processInfo.environment["DRAWKIT_BENCH_RENDER"] != nil
    }

    func runRenderBenchmark() {
        guard let model else {
            renderReport("no model")
            NSApp.terminate(nil)
            return
        }
        let side = 800
        model.reset(width: side, height: side)
        model.setBrush(.ink)
        model.setDeferredRaster(true)
        let ink = Ink(red: 0, green: 0, blue: 0, alpha: 1)
        paint(model, y: 250, scale: .canvas, ink: ink)
        paint(model, y: 550, scale: .screen, ink: ink)
        model.setDeferredRaster(false)
        model.isDirty = false
        model.discardAutosave()

        for (name, thickness) in [("recorded", RenderThickness.recorded),
                                  ("screen", .screen), ("canvas", .canvas)] {
            let spec = OutputSpec.scaled(width: side, height: side, percent: 50,
                                         thickness: thickness)
            guard let pixels = model.renderTape(at: spec) else {
                renderReport("\(name): render failed")
                continue
            }
            let canvasRow = thicknessOf(pixels, spec: spec, at: 125)
            let screenRow = thicknessOf(pixels, spec: spec, at: 275)
            renderReport("\(name) canvas-drawn \(canvasRow) px, screen-drawn \(screenRow) px")
        }
        checkPicking(model)
        checkStrokeEdit(model)
        checkClipboard(model)
        checkPreview(model)
        model.isDirty = false
        model.discardAutosave()
        NSApp.terminate(nil)
    }

    /// A click with the select tool has to find the recorded stroke under it, and a click on
    /// empty paper has to clear the selection.
    private func checkPicking(_ model: PaintModel) {
        guard let window else { return }
        tool = .select
        func click(_ point: CGPoint) {
            let location = convert(viewPoint(point), to: nil)
            guard let down = NSEvent.mouseEvent(
                with: .leftMouseDown, location: location, modifierFlags: [],
                timestamp: ProcessInfo.processInfo.systemUptime,
                windowNumber: window.windowNumber, context: nil, eventNumber: 0,
                clickCount: 1, pressure: 1),
                let up = NSEvent.mouseEvent(
                    with: .leftMouseUp, location: location, modifierFlags: [],
                    timestamp: ProcessInfo.processInfo.systemUptime,
                    windowNumber: window.windowNumber, context: nil, eventNumber: 0,
                    clickCount: 1, pressure: 1)
            else { return }
            mouseDown(with: down)
            mouseUp(with: up)
        }
        click(CGPoint(x: 400, y: 250))
        if let selected = selectedStroke {
            renderReport("pick group \(selected.group) bounds \(selected.bounds)")
        } else {
            renderReport("pick nothing at the stroke")
        }
        click(CGPoint(x: 400, y: 700))
        renderReport(selectedStroke == nil ? "pick cleared on empty paper"
                                           : "pick survived a click on empty paper")
        tool = .brush
    }

    /// Recolour and delete the stroke picked above, with undo between each step.
    private func checkStrokeEdit(_ model: PaintModel) {
        let probe = CGPoint(x: 400, y: 250)
        let group = model.strokeGroup(at: probe)
        guard group != 0 else {
            renderReport("edit: no stroke under the probe")
            return
        }
        let red = Ink(red: 1, green: 0, blue: 0, alpha: 1)
        renderReport("edit before \(layerPixel(model, probe))")
        guard model.recolorStroke(group, ink: red) else {
            renderReport("edit: recolour refused")
            return
        }
        renderReport("edit recoloured \(layerPixel(model, probe))")
        model.undo()
        renderReport("edit undone \(layerPixel(model, probe))")
        model.redo()
        renderReport("edit redone \(layerPixel(model, probe))")
        guard model.deleteStroke(group) else {
            renderReport("edit: delete refused")
            return
        }
        renderReport("edit deleted \(layerPixel(model, probe))")
        model.undo()
        renderReport("edit delete undone \(layerPixel(model, probe))")
    }

    /// A copy has to reach the pasteboard, and a paste has to become a layer with the ink in it.
    private func checkClipboard(_ model: PaintModel) {
        model.selectAll()
        renderReport("clipboard copy: \(model.copyPixels() ? "ok" : "failed")")
        let pasted = model.pastePixels()
        renderReport("clipboard paste: \(pasted ? "ok" : "failed")")
        if pasted {
            let pixel = layerPixel(model, CGPoint(x: 400, y: 250))
            renderReport("clipboard pasted pixel \(pixel)")
            model.undo()
            renderReport("clipboard paste undone, layers \(model.layers.count)")
        }
        model.clearSelection()
    }

    /// The timeline has to present the replay compositor on the surface, and leaving it has to
    /// show the document again without a rebuild.
    private func checkPreview(_ model: PaintModel) {
        guard model.recordingMs > 0 else {
            renderReport("preview: no tape")
            return
        }
        model.previewMs = model.recordingMs
        model.refresh()
        presentSurface(force: true, upload: false)
        renderReport("preview: present code \(lastPresentCode)")
        model.previewMs = nil
        model.refresh()
        presentSurface(force: true, upload: false)
        renderReport("preview live: present code \(lastPresentCode)")
    }

    private func layerPixel(_ model: PaintModel, _ point: CGPoint) -> String {
        var bytes = [UInt8](repeating: 0, count: 4)
        _ = model.withLayerPixels(index: model.activeLayer, x: Int(point.x),
                                  y: Int(point.y), width: 1, height: 1) { source in
            bytes = [source[0], source[1], source[2], source[3]]
            return true
        }
        return "\(bytes[0]),\(bytes[1]),\(bytes[2]),\(bytes[3])"
    }

    private func paint(_ model: PaintModel, y: CGFloat, scale: BrushScale, ink: Ink) {
        model.brushScale = scale
        model.beginStroke()
        model.stroke(from: CGPoint(x: 200, y: y), to: CGPoint(x: 600, y: y),
                     radius: 20, pressure: 1, ink: ink)
        model.endStroke()
    }

    /// The painted height of the horizontal stroke around `row` in the output.
    private func thicknessOf(_ pixels: [UInt8], spec: OutputSpec, at row: Int) -> Int {
        let column = spec.width / 2
        let low = max(0, row - 40)
        let high = min(spec.height - 1, row + 40)
        var first = -1
        var last = -1
        for y in low...high {
            let alpha = pixels[(y * spec.width + column) * 4 + 3]
            if alpha > 32 {
                if first < 0 { first = y }
                last = y
            }
        }
        return first < 0 ? 0 : last - first + 1
    }

    private func renderReport(_ message: String) {
        FileHandle.standardError.write(Data(("DRAWKIT_RENDER \(message)\n").utf8))
    }
}
