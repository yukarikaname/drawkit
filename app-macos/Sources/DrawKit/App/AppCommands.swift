// The menu bar: File and Edit are the standard document commands, Tools mirrors the rail, View
// carries zoom and the reference overlay, Layer carries the layer stack, and Painting carries the
// tape and the GPU preview. The canvas keeps the single-letter tool shortcuts so typing in a text
// field is never intercepted by a bare menu key.

import SwiftUI
import AppKit

struct AppCommands: Commands {
    let model: PaintModel
    let shortcuts: ShortcutStore
    let workspace: Workspace
    let settings: AppSettings

    var body: some Commands {
        CommandGroup(replacing: .appInfo) {
            Button("About DrawKit") {
                NSApplication.shared.orderFrontStandardAboutPanel(options: [
                    .credits: NSAttributedString(
                        string: "A practical painting kit.\nThe same core and the same pixels on every screen.")
                ])
            }
        }

        CommandGroup(after: .newItem) {
            Button("Open Project or Image…") { model.openProject() }
                .keyboardShortcut(shortcuts.keyEquivalent(for: .open),
                                  modifiers: shortcuts.eventModifiers(for: .open))
            Button("Save Project") { model.saveProject() }
                .keyboardShortcut(shortcuts.keyEquivalent(for: .save),
                                  modifiers: shortcuts.eventModifiers(for: .save))
            Button("Save Project As…") { model.saveProjectAs() }
                .keyboardShortcut("s", modifiers: [.command, .shift])
            Menu("Open Recent") {
                let recent = RecentDocuments.all()
                ForEach(Array(recent.prefix(8)), id: \.self) { url in
                    Button(url.lastPathComponent) { _ = model.openDocument(url) }
                }
                if !recent.isEmpty {
                    Divider()
                    Button("Clear Menu") { RecentDocuments.clear() }
                }
            }
            Button("Export PNG…") { model.exportPNG() }
                .keyboardShortcut(shortcuts.keyEquivalent(for: .export),
                                  modifiers: shortcuts.eventModifiers(for: .export))
            Button("Export JPEG…") { model.exportJPEG() }
            Button("Export WebP…") { model.exportWebP() }
            Button("Export EXR…") { model.exportEXR() }
            Button("Export Timelapse…") { model.exportTimelapse() }
            Divider()
            Button("Import Krita or PSD…") { model.importDocument() }
            Button("Export Krita…") { model.exportKra() }
            Button("Export Photoshop…") { model.exportPsd() }
            Button("Export Layers as PNG…") { model.exportLayers() }
        }

        CommandGroup(replacing: .undoRedo) {
            Button("Undo") { model.undo() }
                .keyboardShortcut(shortcuts.keyEquivalent(for: .undo),
                                  modifiers: shortcuts.eventModifiers(for: .undo))
                .disabled(!model.canUndo)
            Button("Redo") { model.redo() }
                .keyboardShortcut(shortcuts.keyEquivalent(for: .redo),
                                  modifiers: shortcuts.eventModifiers(for: .redo))
                .disabled(!model.canRedo)
        }

        CommandMenu("Tools") {
            toolToggle("Brush", .brush)
            toolToggle("Eraser", .eraser)
            toolToggle("Smudge", .smudge)
            toolToggle("Fill", .fill)
            toolToggle("Eyedropper", .eyedropper)
            Divider()
            Menu("Brush Preset") {
                ForEach(BrushKind.allCases) { kind in
                    Button(kind.title) { workspace.brush = kind }
                }
            }
            Divider()
            Button("Smaller Brush") { workspace.size = max(1, workspace.size - 2) }
            Button("Larger Brush") { workspace.size = min(48, workspace.size + 2) }
            Divider()
            Button("Default Colour") { workspace.color = .black }
            Button("White") { workspace.color = .white }
        }

        CommandGroup(after: .toolbar) {
            Button("Zoom In") { workspace.zoom = min(32, workspace.zoom * 1.25) }
                .keyboardShortcut("+", modifiers: .command)
            Button("Zoom Out") { workspace.zoom = max(0.1, workspace.zoom / 1.25) }
                .keyboardShortcut("-", modifiers: .command)
            Button("Actual Size") { workspace.zoom = 1 }
                .keyboardShortcut("0", modifiers: .command)
            Button("Fit Canvas") { workspace.fitRequest &+= 1 }
                .keyboardShortcut("0", modifiers: [.command, .shift])
            Divider()
        }

        CommandMenu("Layer") {
            Button("Add Layer") { model.addLayer() }
            Button("Add Group") { model.addGroup() }
            Button("Add Fill Layer") { model.addFillLayer(workspace.color) }
            Button("Remove Layer") { model.removeLayer() }
            Button("Layer Above") { model.stepLayer(1) }
                .keyboardShortcut("]", modifiers: [.option])
            Button("Layer Below") { model.stepLayer(-1) }
                .keyboardShortcut("[", modifiers: [.option])
            Button("Remove Paper from Layer") { model.removePaperFromLayer() }
            Button("Clear Layer") { model.clear() }
                .keyboardShortcut(shortcuts.keyEquivalent(for: .clearLayer),
                                  modifiers: shortcuts.eventModifiers(for: .clearLayer))
            Divider()
            Toggle("Visible", isOn: Binding(
                get: { model.layers.first(where: { $0.id == model.activeLayer })?.visible ?? false },
                set: { model.setVisible(index: model.activeLayer, visible: $0) }
            ))
        }
    }

    private func toolToggle(_ title: String, _ tool: Tool) -> some View {
        Toggle(title, isOn: Binding(
            get: { workspace.tool == tool },
            set: { selected in
                if selected { workspace.tool = tool }
            }
        ))
    }
}
