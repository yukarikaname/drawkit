// Brush files. The core owns the parameters; the shell only chooses a path and a name.

import AppKit
import Foundation
import UniformTypeIdentifiers

extension PaintModel {
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
