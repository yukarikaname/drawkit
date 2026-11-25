// Brush files. The core owns the parameters; the shell only chooses a path and a name.

import AppKit
import Foundation
import UniformTypeIdentifiers

extension PaintModel {
    /// Read the current parameters back and refresh the small stroke preview.
    func refreshBrush() {
        guard handle != nil else { return }
        var values = [Double](repeating: 0, count: 9)
        for id in 0..<9 {
            values[id] = Double(dk_brush_param(handle, UInt32(id)))
        }
        brushParameters = values
        refreshBrushPreview()
    }

    func refreshBrushPreviews() {
        guard handle != nil else { return }
        var previews: [CGImage?] = []
        previews.reserveCapacity(9)
        for kind in 0..<9 {
            previews.append(renderBrushPreview(kind: Int32(kind), width: 168, height: 40))
        }
        brushSlotPreviews = previews
    }

    private func renderBrushPreview(kind: Int32, width: Int, height: Int) -> CGImage? {
        let buffer = PixelBuffer(count: width * height * 4)
        guard dk_brush_preview(handle, kind, UInt32(width), UInt32(height),
                               buffer.pointer) != 0 else { return nil }
        return makeImage(buffer: buffer)
    }

    func resetBrush() {
        guard handle != nil else { return }
        dk_reset_brush(handle)
        refreshBrush()
        refreshBrushPreviews()
    }

    func setBrushParameter(_ id: Int, _ value: Double) {
        guard handle != nil, (0..<9).contains(id) else { return }
        dk_set_brush_param(handle, UInt32(id), Float(value))
        var next = brushParameters
        next[id] = Double(dk_brush_param(handle, UInt32(id)))
        brushParameters = next
        refreshBrushPreview()
        let slot = Int(dk_brush(handle))
        if (0..<9).contains(slot) {
            var previews = brushSlotPreviews
            previews[slot] = renderBrushPreview(kind: Int32(slot), width: 168, height: 40)
            brushSlotPreviews = previews
        }
    }

    func refreshBrushPreview() {
        guard handle != nil else { return }
        let width = 240
        let height = 56
        let buffer = PixelBuffer(count: width * height * 4)
        if dk_brush_preview(handle, -1, UInt32(width), UInt32(height),
                            buffer.pointer) != 0 {
            brushPreview = makeImage(buffer: buffer)
        }
    }

    /// Import a MyPaint `.myb` and make it current. Returns the kind so the workspace tray
    /// can follow without calling `setBrush`, which would reset the imported parameters.
    @discardableResult
    func importBrush() -> BrushKind? {
        let panel = NSOpenPanel()
        panel.allowedContentTypes = [UTType(filenameExtension: "myb") ?? .data]
        panel.allowsMultipleSelection = false
        panel.prompt = "Import"
        guard panel.runModal() == .OK, let url = panel.url else { return nil }
        let kind = url.path.withCString { dk_import_brush(handle, $0) }
        guard kind >= 0, let brush = BrushKind(rawValue: Int(kind)) else {
            NSSound.beep()
            flash("Could not import brush", error: true)
            return nil
        }
        brushKind = brush
        refreshBrush()
        refreshBrushPreviews()
        flash("Imported brush")
        return brush
    }

    func exportBrush() {
        let panel = NSSavePanel()
        panel.allowedContentTypes = [UTType(filenameExtension: "myb") ?? .data]
        panel.nameFieldStringValue = "\(brushKind.title).myb"
        panel.prompt = "Export"
        guard panel.runModal() == .OK, let url = panel.url else { return }
        let saved = url.path.withCString { dk_export_brush(handle, $0) }
        if saved != 0 {
            flash("Exported brush")
        } else {
            NSSound.beep()
            flash("Could not export brush", error: true)
        }
    }
}
