// The iPad menu bar and hardware-keyboard shortcuts. The same actions as the ellipsis menu, plus
// the standard document and edit commands, exposed through SwiftUI's command system.

import SwiftUI

struct DrawKitCommands: Commands {
    let studio: Studio
    let settings: AppSettings

    var body: some Commands {
        CommandGroup(replacing: .newItem) {
            Button("Import Krita or PSD…") { studio.showImport = true }
                .keyboardShortcut("o", modifiers: .command)
            Button("PencilKit sketch…") { studio.showPencilKit = true }
            Divider()
            Button("Save Project…") { studio.prepareExport("dkp") }
                .keyboardShortcut("s", modifiers: .command)
            Button("Export Krita…") { studio.prepareExport("kra") }
                .keyboardShortcut("e", modifiers: [.command, .shift])
            Button("Export Photoshop…") { studio.prepareExport("psd") }
            Button("Export EXR…") { studio.prepareExport("exr") }
            Button("Save Stroke Tape…") { studio.prepareExport("dkp") }
        }

        CommandGroup(after: .pasteboard) {
            Button("Cut") { _ = studio.cutPixels() }
                .keyboardShortcut("x", modifiers: .command)
            Button("Copy") { _ = studio.copyPixels() }
                .keyboardShortcut("c", modifiers: .command)
            Button("Copy Merged") { _ = studio.copyPixels(merged: true) }
                .keyboardShortcut("c", modifiers: [.command, .shift])
            Button("Paste") { _ = studio.pastePixels() }
                .keyboardShortcut("v", modifiers: .command)
        }

        CommandGroup(replacing: .undoRedo) {
            Button("Undo") { studio.undo() }
                .keyboardShortcut("z", modifiers: .command)
                .disabled(!studio.canUndo)
            Button("Redo") { studio.redo() }
                .keyboardShortcut("z", modifiers: [.command, .shift])
                .disabled(!studio.canRedo)
        }

        CommandMenu("Tools") {
            toolButton(.brush, key: "b")
            toolButton(.eraser, key: "e")
            toolButton(.fill, key: "f")
            Divider()
            Button("Clear Layer") { studio.clearLayer() }
                .keyboardShortcut(.delete, modifiers: .command)
            Divider()
            Menu("Mirror") {
                mirrorButton("Off", mode: 0)
                mirrorButton("Horizontal", mode: 1)
                mirrorButton("Vertical", mode: 2)
                mirrorButton("Both", mode: 3)
            }
        }

        CommandMenu("View") {
            Button("Layers") { studio.showLayers = true }
                .keyboardShortcut("l", modifiers: [.command, .shift])
            Button("Colours") { studio.showColours = true }
                .keyboardShortcut("c", modifiers: [.command, .shift])
            Divider()
            Button("Settings…") { studio.showSettings = true }
                .keyboardShortcut(",", modifiers: .command)
            Button("Welcome Tour") { settings.hasCompletedOnboarding = false }
        }
    }

    private func toolButton(_ tool: IOSTool, key: KeyEquivalent) -> some View {
        Button(tool.rawValue) { studio.selectTool(tool) }
            .keyboardShortcut(key, modifiers: [])
    }

    private func mirrorButton(_ title: String, mode: Int) -> some View {
        Button(title) { studio.mirror = mode }
    }
}
