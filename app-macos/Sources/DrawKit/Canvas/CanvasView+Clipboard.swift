// The standard Edit menu actions. AppKit sends these selectors down the responder chain, so the
// canvas answers them whenever it has focus and a text field keeps its own copy and paste.

import AppKit

extension CanvasView {
    @objc func copy(_ sender: Any?) {
        guard let model, model.copyPixels() else { NSSound.beep(); return }
        model.flash(String(localized: "Copied"))
    }

    @objc func cut(_ sender: Any?) {
        guard let model, model.cutPixels() else { NSSound.beep(); return }
        presentSurface(force: true)
        model.flash(String(localized: "Cut"))
    }

    @objc func paste(_ sender: Any?) {
        guard let model, model.pastePixels() else { NSSound.beep(); return }
        presentSurface(force: true, upload: true)
        model.flash(String(localized: "Pasted to a new layer"))
    }

    @objc func delete(_ sender: Any?) {
        guard let model else { return }
        model.clear()
        presentSurface(force: true)
    }

    /// Copy the composed frame instead of the active layer.
    @objc func copyMerged(_ sender: Any?) {
        guard let model, model.copyPixels(merged: true) else { NSSound.beep(); return }
        model.flash(String(localized: "Copied"))
    }
}
