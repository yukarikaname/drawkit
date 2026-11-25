// Application lifecycle. The window can be closed with unsaved work, so quitting asks once and
// leaves a recovery copy when the answer is "don't save".

import AppKit

@MainActor
final class AppDelegate: NSObject, NSApplicationDelegate {
    weak var model: PaintModel?

    func applicationShouldTerminate(_ sender: NSApplication) -> NSApplication.TerminateReply {
        // The benchmarks draw without a person to answer a save prompt.
        if CanvasView.automatedRun {
            return .terminateNow
        }
        guard let model, model.isDirty else { return .terminateNow }
        model.autosaveIfNeeded()
        let alert = NSAlert()
        alert.messageText = "Save changes before quitting?"
        alert.informativeText = "The painting has unsaved changes. A recovery copy is kept if you quit without saving."
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
