// One row in the layer palette, with the gestures that belong to it.

import SwiftUI
import AppKit

/// One row in the palette: the eye, the thumbnail, the name and opacity. A click selects it, a
/// press-and-drag lifts it to reorder, a double-click or Return renames it, and the native list
/// swipe reveals Delete.
struct LayerRow: View {
    let model: PaintModel
    let layer: LayerInfo

    @State private var hovering = false
    @State private var lifting = false
    @State private var lift: CGFloat = 0
    @State private var renaming = false
    @State private var renameText = ""
    @FocusState private var rowFocused: Bool
    @FocusState private var renameFocused: Bool
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    private var selected: Bool { model.activeLayer == layer.id }
    private var group: Bool { layer.kind == .group }
    private var indent: CGFloat { CGFloat(model.depth(of: layer)) * 10 + 1 }
    private var thumbnail: CGImage? {
        guard !group, layer.id < model.thumbnails.count else { return nil }
        return model.thumbnails[layer.id]
    }

    var body: some View {
        rowContent
            .offset(y: lift + (lifting ? -2 : 0))
            .scaleEffect(lifting ? 1.03 : 1)
            .opacity(lifting ? 0.97 : 1)
            .shadow(color: .black.opacity(lifting ? 0.4 : 0), radius: 14, y: 7)
            .zIndex(lifting ? 1 : 0)
            .animation(.easeOut(duration: 0.12), value: lifting)
            .contentShape(Rectangle())
            .onTapGesture(count: 2) { beginRename() }
            .onTapGesture { select() }
            .simultaneousGesture(liftGesture)
            .focusable()
            .focused($rowFocused)
            .focusEffectDisabled()
            .onKeyPress(.return) {
                beginRename()
                return .handled
            }
            .onKeyPress(KeyEquivalent("\u{F705}")) {
                beginRename()
                return .handled
            }
    }

    private var rowContent: some View {
        HStack(spacing: 5) {
            if group {
                Button {
                    model.setExpanded(index: layer.id, expanded: !layer.expanded)
                } label: {
                    Image(systemName: layer.expanded ? "chevron.down" : "chevron.right")
                        .font(.caption2.weight(.semibold))
                        .frame(width: 12)
                }
                .buttonStyle(.borderless)
            } else {
                Spacer().frame(width: 12)
            }

            Button {
                model.setVisible(index: layer.id, visible: !layer.visible)
            } label: {
                Image(systemName: layer.visible ? "eye" : "eye.slash")
                    .font(.system(size: 12))
                    .foregroundStyle(layer.visible ? Brand.muted : Brand.vermilion.opacity(0.75))
                    .frame(width: 20, height: 28)
            }
            .buttonStyle(.borderless)
            .help(layer.visible ? "Hide" : "Show")

            thumbnailView

            if layer.colorTag != 0 {
                Circle()
                    .fill(tagColor(layer.colorTag))
                    .frame(width: 7, height: 7)
                    .help("Colour tag")
            }

            if renaming {
                TextField("", text: $renameText)
                    .textFieldStyle(.plain)
                    .font(.system(size: 12, weight: group ? .semibold : .regular))
                    .foregroundStyle(Brand.paper)
                    .focused($renameFocused)
                    .onAppear { renameFocused = true }
                    .onSubmit { commitRename() }
                    .onExitCommand { renaming = false }
                    .onChange(of: renameFocused) { _, focused in
                        if !focused, renaming { commitRename() }
                    }
                    .layoutPriority(1)
            } else {
                Text(layer.name)
                    .font(.system(size: 12, weight: group ? .semibold : .regular))
                    .foregroundStyle(layer.locked ? Brand.muted : Brand.paper)
                    .lineLimit(1)
                    .truncationMode(.middle)
                    .layoutPriority(1)
            }
            Spacer(minLength: 2)

            if hovering || selected {
                Menu {
                    layerCommands
                } label: {
                    Image(systemName: "ellipsis")
                        .font(.system(size: 12, weight: .semibold))
                        .foregroundStyle(Brand.muted)
                        .frame(width: 22, height: 28)
                }
                .menuStyle(.borderlessButton)
                .menuIndicator(.hidden)
                .fixedSize()
                .help("Layer options")
            }

            Text("\(Int(layer.opacity * 100))%")
                .font(.caption2.monospacedDigit())
                .foregroundStyle(Brand.muted)
                .frame(width: 32, alignment: .trailing)
        }
        .padding(.leading, indent)
        .padding(.horizontal, 4)
        .frame(height: 40)
        .background(
            RoundedRectangle(cornerRadius: 9, style: .continuous)
                .fill(lifting ? Brand.raised
                      : selected ? Brand.vermilion.opacity(0.20)
                      : Color.white.opacity(hovering ? 0.04 : 0))
        )
        .overlay(
            RoundedRectangle(cornerRadius: 9, style: .continuous)
                .stroke(Brand.vermilion.opacity(lifting ? 0.9 : 0),
                        lineWidth: lifting ? 2 : 1.5)
        )
        .onHover { inside in
            withAnimation(.easeOut(duration: 0.12)) { hovering = inside }
        }
    }

    @ViewBuilder
    private var thumbnailView: some View {
        if group {
            Image(systemName: layer.expanded ? "folder.fill" : "folder")
                .font(.system(size: 15))
                .foregroundStyle(Brand.vermilion.opacity(0.85))
                .frame(width: 32, height: 32)
        } else if let thumbnail {
            Image(decorative: thumbnail, scale: 1)
                .resizable()
                .interpolation(.none)
                .frame(width: 32, height: 32)
                .background(Brand.ink.opacity(0.55))
                .clipShape(RoundedRectangle(cornerRadius: 5, style: .continuous))
                .overlay(RoundedRectangle(cornerRadius: 5, style: .continuous)
                    .stroke(Color.white.opacity(0.12), lineWidth: 1))
        } else {
            RoundedRectangle(cornerRadius: 5, style: .continuous)
                .fill(Brand.ink.opacity(0.45))
                .frame(width: 32, height: 32)
        }
    }

    /// Press-and-drag is the same motion a Force Touch lift uses. SwiftUI has no pressure event,
    /// so the long press starts the lift and the sequenced drag follows the pointer.
    private var liftGesture: some Gesture {
        LongPressGesture(minimumDuration: 0.15, maximumDistance: 10)
            .sequenced(before: DragGesture(minimumDistance: 0, coordinateSpace: .global))
            .onChanged { value in
                switch value {
                case .first(true):
                    beginLift()
                case .second(true, let drag?):
                    if !lifting { beginLift() }
                    lift = drag.translation.height
                default:
                    break
                }
            }
            .onEnded { _ in endLift() }
    }

    private func tagColor(_ tag: Int) -> Color {
        switch tag {
        case 1: Brand.vermilion
        case 2: .orange
        case 3: .yellow
        case 4: .green
        case 5: .blue
        case 6: .purple
        default: .clear
        }
    }

    private func tagName(_ tag: Int) -> String {
        switch tag {
        case 1: "Red"
        case 2: "Orange"
        case 3: "Yellow"
        case 4: "Green"
        case 5: "Blue"
        case 6: "Purple"
        default: "None"
        }
    }

    private func select() {
        rowFocused = true
        withAnimation(Motion.select.resolving(reduceMotion: reduceMotion)) {
            model.selectLayer(layer.id)
        }
    }

    private func beginLift() {
        guard !lifting else { return }
        // Selecting the row rebuilds the palette and can cancel the gesture mid-flight, so the
        // active layer changes only after the drop.
        lift = 0
        lifting = true
        NSHapticFeedbackManager.defaultPerformer.perform(.generic, performanceTime: .now)
    }

    private func endLift() {
        guard lifting else { return }
        let steps = Int((-lift / 42).rounded())
        lifting = false
        lift = 0
        model.selectLayer(layer.id)
        guard steps != 0 else { return }
        withAnimation(Motion.panel.resolving(reduceMotion: reduceMotion)) {
            model.moveLayer(layer.id, steps: steps)
        }
        NSHapticFeedbackManager.defaultPerformer.perform(.alignment, performanceTime: .now)
    }

    private func beginRename() {
        select()
        renameText = layer.name
        renaming = true
        DispatchQueue.main.async { renameFocused = true }
    }

    private func commitRename() {
        guard renaming else { return }
        renaming = false
        let name = renameText.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !name.isEmpty, name != layer.name else { return }
        model.renameLayer(layer.id, name: name)
    }

    @ViewBuilder
    private var layerCommands: some View {
        Toggle("Clip to layer below", isOn: Binding(
            get: { layer.clipping },
            set: { model.setClipping(index: layer.id, clipped: $0) }
        ))
        Toggle("Lock transparency", isOn: Binding(
            get: { layer.alphaLocked },
            set: { model.setAlphaLocked(index: layer.id, locked: $0) }
        ))
        Toggle("Lock layer", isOn: Binding(
            get: { layer.locked },
            set: { model.setLocked(index: layer.id, locked: $0) }
        ))
        Divider()
        Menu("Colour tag") {
            Button("None") { model.setTag(index: layer.id, tag: 0) }
            ForEach(1...6, id: \.self) { tag in
                Button(tagName(tag)) { model.setTag(index: layer.id, tag: tag) }
            }
        }
        Button("Rename") { beginRename() }
        Button("Remove Paper from Layer") { model.removePaperFromLayer() }
        Button("Clear Layer") { model.clear() }
    }
}
