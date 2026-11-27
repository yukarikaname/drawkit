// Application lifecycle. The window can be closed with unsaved work, so quitting asks once and
// leaves a recovery copy when the answer is "don't save".
//
// The delegate also owns the two paths the Finder and other apps use to call into DrawKit: the
// Services item declared in Info.plist and the file-open event that "Open With" produces. Both
// arrive before SwiftUI has necessarily run `onAppear`, so the model is attached lazily and any
// open request that beat it waits here.

import AppKit

@MainActor
final class AppDelegate: NSObject, NSApplicationDelegate {
    weak var model: PaintModel?
    private var pendingOpen: [URL] = []

    func applicationDidFinishLaunching(_ notification: Notification) {
        // Finder and Preview look the service up on the running application, not through the
        // responder chain. The provider is the object that implements the NSServices selector.
        NSApp.servicesProvider = self
        NSUpdateDynamicServices()
    }

    /// The app-level model is created with the scene, which can be later than an open event from
    /// LaunchServices. Flush what arrived first once the model exists.
    func attach(model: PaintModel) {
        self.model = model
        guard !pendingOpen.isEmpty else { return }
        let urls = pendingOpen
        pendingOpen.removeAll()
        open(urls, with: model)
    }

    // MARK: Files

    func application(_ application: NSApplication, open urls: [URL]) {
        guard let model else {
            pendingOpen.append(contentsOf: urls)
            return
        }
        open(urls, with: model)
    }

    /// One window, one painting: the first URL wins. The core's open path already knows which
    /// extensions it can read, and reports the rest through the status line.
    private func open(_ urls: [URL], with model: PaintModel) {
        guard let url = urls.first else { return }
        _ = model.openDocument(url)
    }

    // MARK: Services

    /// `Import as New Layer` from Finder, Safari or Preview. The service is sent on the main
    /// thread; the error pointer is how AppKit shows a failure in the Finder alert.
    @objc func importAsNewLayer(
        _ board: NSPasteboard, userData: String,
        error: AutoreleasingUnsafeMutablePointer<NSString?>
    ) {
        guard let model else {
            error.pointee = "Open a DrawKit painting first." as NSString
            return
        }
        guard let (image, name) = image(from: board) else {
            error.pointee = "The selection does not contain an image or an image file." as NSString
            return
        }
        guard model.addImageLayer(fitted: image, name: name) else {
            error.pointee = "DrawKit could not add that image." as NSString
            return
        }
        model.flash(String(localized: "Imported \(name)"))
    }

    /// A file selection and an in-memory image are different pasteboard shapes. Accept both:
    /// Finder and Preview send file URLs, Safari can send TIFF/PNG data for a selected picture.
    private func image(from board: NSPasteboard) -> (CGImage, String)? {
        let urls =
            board.readObjects(
                forClasses: [NSURL.self],
                options: [.urlReadingFileURLsOnly: true]) as? [URL]
        if let url = urls?.first, let image = NSImage(contentsOf: url),
            let cgImage = cgImage(from: image)
        {
            return (cgImage, url.deletingPathExtension().lastPathComponent)
        }
        if let image = NSImage(pasteboard: board), let cgImage = cgImage(from: image) {
            return (cgImage, String(localized: "Imported Image"))
        }
        return nil
    }

    private func cgImage(from image: NSImage) -> CGImage? {
        var rect = CGRect(origin: .zero, size: image.size)
        return image.cgImage(forProposedRect: &rect, context: nil, hints: nil)
    }

    // MARK: Quit

    func applicationShouldTerminate(_ sender: NSApplication) -> NSApplication.TerminateReply {
        // The benchmarks draw without a person to answer a save prompt.
        if CanvasView.automatedRun {
            return .terminateNow
        }
        guard let model, model.isDirty else { return .terminateNow }
        model.autosaveIfNeeded()
        let alert = NSAlert()
        alert.messageText = "Save changes before quitting?"
        alert.informativeText =
            "The painting has unsaved changes. A recovery copy is kept if you quit without saving."
        alert.addButton(withTitle: "Save…")
        alert.addButton(withTitle: "Don't Save")
        alert.addButton(withTitle: "Cancel")
        switch alert.runModal() {
        case .alertFirstButtonReturn:
            model.saveProject()
            return model.isDirty ? .terminateCancel : .terminateNow
        case .alertSecondButtonReturn:
            // "Don't Save" means discard, so do not leave a recovery copy that asks again on the
            // next launch. A crash during the session still leaves one before this point.
            model.discardAutosave()
            model.isDirty = false
            return .terminateNow
        default:
            return .terminateCancel
        }
    }
}
