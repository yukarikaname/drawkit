// The standard edit actions for the iPad canvas: the system menu (long press with a finger), the
// hardware keyboard, and the menu bar all route here.

import UIKit

extension SketchCanvasView {
    override func copy(_ sender: Any?) {
        guard let studio, studio.copyPixels() else { return }
    }

    override func cut(_ sender: Any?) {
        guard let studio, studio.cutPixels() else { return }
        redraw()
    }

    override func paste(_ sender: Any?) {
        guard let studio, studio.pastePixels() else { return }
        presentSurface(force: true, upload: true)
        redraw()
    }

    override func delete(_ sender: Any?) {
        studio?.clearLayer()
        redraw()
    }

    /// Copy the composed frame instead of the active layer. Reached from the menu, not the keyboard.
    @objc func copyMerged(_ sender: Any?) {
        guard let studio, studio.copyPixels(merged: true) else { return }
    }

    override var keyCommands: [UIKeyCommand]? {
        [
            UIKeyCommand(input: "c", modifierFlags: .command, action: #selector(copy(_:))),
            UIKeyCommand(input: "c", modifierFlags: [.command, .shift],
                         action: #selector(copyMerged(_:))),
            UIKeyCommand(input: "x", modifierFlags: .command, action: #selector(cut(_:))),
            UIKeyCommand(input: "v", modifierFlags: .command, action: #selector(paste(_:))),
        ]
    }

    /// A finger held still on the canvas opens the edit menu; moving paints as usual.
    func installEditMenu() {
        let interaction = UIEditMenuInteraction(delegate: self)
        addInteraction(interaction)
        editMenuInteraction = interaction

        let press = UILongPressGestureRecognizer(target: self, action: #selector(handleLongPress(_:)))
        press.minimumPressDuration = 0.45
        press.allowedTouchTypes = [NSNumber(value: UITouch.TouchType.direct.rawValue)]
        addGestureRecognizer(press)
    }

    @objc private func handleLongPress(_ gesture: UILongPressGestureRecognizer) {
        guard gesture.state == .began else { return }
        cancelStroke()
        let point = gesture.location(in: self)
        editMenuInteraction?.presentEditMenu(
            with: UIEditMenuConfiguration(identifier: nil, sourcePoint: point))
    }
}

extension SketchCanvasView: @preconcurrency UIEditMenuInteractionDelegate {
    func editMenuInteraction(
        _ interaction: UIEditMenuInteraction,
        menuFor configuration: UIEditMenuConfiguration,
        suggestedActions: [UIMenuElement]
    ) -> UIMenu? {
        UIMenu(children: [
            UIAction(title: String(localized: "Cut")) { [weak self] _ in
                self?.cut(nil)
            },
            UIAction(title: String(localized: "Copy")) { [weak self] _ in
                self?.copy(nil)
            },
            UIAction(title: String(localized: "Copy Merged")) { [weak self] _ in
                self?.copyMerged(nil)
            },
            UIAction(title: String(localized: "Paste")) { [weak self] _ in
                self?.paste(nil)
            },
        ])
    }
}
