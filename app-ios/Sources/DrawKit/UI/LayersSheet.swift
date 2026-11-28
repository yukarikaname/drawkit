// The layer sheet: thumbnails, visibility, selection and the active layer's opacity.

import SwiftUI
import UIKit

struct LayersSheet: View {
    @Bindable var studio: Studio
    @Environment(\.dismiss) private var dismiss
    @State private var thumbnails: [Int: UIImage] = [:]
    @State private var query = ""
    @State private var renameLayer: IOSLayer?
    @State private var renameText = ""
    @Environment(\.layoutDirection) private var layoutDirection
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            HStack(spacing: 10) {
                BrandMark(size: 24, line: Brand.paper)
                Text("Layers")
                    .font(.system(size: 24, weight: .bold, design: .rounded))
                    .foregroundStyle(Brand.paper)
                Spacer()
                icon("plus") {
                    studio.addLayer()
                    refreshThumbnails()
                }
                icon("folder.badge.plus") {
                    studio.addGroup()
                    refreshThumbnails()
                }
                icon("paintbrush.fill") {
                    studio.addFillLayer()
                    refreshThumbnails()
                }
                icon("minus") {
                    studio.removeLayer()
                    refreshThumbnails()
                }
                Button("Done") { dismiss() }
                    .buttonStyle(.bordered)
                    .tint(Brand.paper)
            }

            TextField("Search layers", text: $query)
                .textFieldStyle(.roundedBorder)

            List {
                ForEach(
                    studio.layerRows().filter {
                        query.isEmpty || $0.name.localizedCaseInsensitiveContains(query)
                    }
                ) { layer in
                    row(layer)
                        .onDrag {
                            guard let data = studio.layerPNGData(index: layer.id) else {
                                return NSItemProvider()
                            }
                            let url = FileManager.default.temporaryDirectory
                                .appendingPathComponent("DrawKit-\(layer.id + 1).png")
                            try? data.write(to: url)
                            return NSItemProvider(contentsOf: url) ?? NSItemProvider()
                        }
                        .listRowInsets(EdgeInsets(top: 2, leading: 0, bottom: 2, trailing: 0))
                        .listRowSeparator(.hidden)
                        .listRowBackground(Color.clear)
                        .swipeActions(edge: .trailing, allowsFullSwipe: true) {
                            Button(role: .destructive) {
                                studio.selectLayer(layer.id)
                                studio.removeLayer()
                                refreshThumbnails()
                            } label: {
                                Label("Delete", systemImage: "trash")
                            }
                        }
                }
            }
            .listStyle(.plain)
            .scrollContentBackground(.hidden)
            .frame(maxHeight: 380)
            .animation(reduceMotion ? nil : .snappy(duration: 0.3), value: studio.layers)

            if let active = studio.layers.first(where: { $0.id == studio.activeLayer }) {
                HStack {
                    Text("Opacity")
                        .font(.system(size: 11))
                        .foregroundStyle(Brand.muted)
                    Spacer()
                    Text("\(Int(active.opacity * 100))%")
                        .font(.system(size: 11).monospacedDigit())
                        .foregroundStyle(Brand.muted)
                }
                Slider(
                    value: Binding(
                        get: { active.opacity },
                        set: { studio.setOpacity(active.id, $0) }
                    ),
                    in: 0...1
                )
                .tint(Brand.chalk)
            }
        }
        .padding(20)
        .background(Brand.carbon)
        .preferredColorScheme(.dark)
        .presentationDetents([.medium, .large])
        .presentationDragIndicator(.visible)
        .onAppear(perform: refreshThumbnails)
        .alert(
            "Rename layer",
            isPresented: Binding(
                get: { renameLayer != nil },
                set: { if !$0 { renameLayer = nil } }
            )
        ) {
            TextField("Name", text: $renameText)
            Button("Rename") {
                if let layer = renameLayer {
                    studio.renameLayer(layer.id, name: renameText)
                }
                renameLayer = nil
            }
            Button("Cancel", role: .cancel) { renameLayer = nil }
        }
    }

    private func row(_ layer: IOSLayer) -> some View {
        HStack(spacing: 10) {
            if layer.isGroup {
                Button {
                    studio.setExpanded(layer.id, !layer.expanded)
                } label: {
                    Image(systemName: disclosureSymbol(layer))
                        .font(.system(size: 11, weight: .bold))
                        .foregroundStyle(Brand.muted)
                        .frame(width: 14)
                }
                .buttonStyle(.plain)
                Image(systemName: "folder.fill")
                    .font(.system(size: 20))
                    .foregroundStyle(Brand.paper.opacity(0.7))
                    .frame(width: 42, height: 42)
                    .background(
                        Color.white.opacity(0.05),
                        in: RoundedRectangle(cornerRadius: 7))
            } else if layer.isFill {
                Color.clear.frame(width: 14)
                Image(systemName: "square.fill")
                    .font(.system(size: 20))
                    .foregroundStyle(Brand.paper.opacity(0.7))
                    .frame(width: 42, height: 42)
                    .background(
                        Color.white.opacity(0.05),
                        in: RoundedRectangle(cornerRadius: 7))
            } else {
                Color.clear.frame(width: 14)
                if let image = thumbnails[layer.id] {
                    Image(uiImage: image)
                        .resizable()
                        .interpolation(.none)
                        .frame(width: 42, height: 42)
                        .background(Brand.paper)
                        .clipShape(RoundedRectangle(cornerRadius: 7))
                } else {
                    RoundedRectangle(cornerRadius: 7)
                        .fill(Brand.paper)
                        .frame(width: 42, height: 42)
                }
            }
            VStack(alignment: .leading, spacing: 1) {
                HStack(spacing: 5) {
                    if layer.reference {
                        Image(systemName: "book.closed.fill")
                            .font(.system(size: 9))
                            .foregroundStyle(Brand.muted)
                    }
                    if layer.tag != 0 {
                        Circle()
                            .fill(tagColor(layer.tag))
                            .frame(width: 6, height: 6)
                    }
                    Text(layer.name)
                        .font(
                            .system(
                                size: 14,
                                weight: layer.id == studio.activeLayer ? .semibold : .regular)
                        )
                        .foregroundStyle(Brand.paper)
                }
                Text("\(Int(layer.opacity * 100))%")
                    .font(.system(size: 10))
                    .foregroundStyle(Brand.muted)
            }
            Spacer()
            Button {
                studio.setVisible(layer.id, !layer.visible)
                refreshThumbnails()
            } label: {
                Image(systemName: layer.visible ? "eye" : "eye.slash")
                    .font(.system(size: 16))
                    .foregroundStyle(Brand.paper.opacity(layer.visible ? 0.75 : 0.35))
                    .frame(width: 38, height: 38)
            }
            .buttonStyle(.plain)
        }
        .padding(6)
        .background(
            RoundedRectangle(cornerRadius: 12)
                .fill(
                    layer.id == studio.activeLayer
                        ? Brand.proof.opacity(0.14) : Color.clear)
        )
        .padding(.leading, CGFloat(layer.depth) * 16)
        .contentShape(Rectangle())
        .onTapGesture { studio.selectLayer(layer.id) }
        .contextMenu {
            Toggle(
                "Reference layer",
                isOn: Binding(
                    get: { layer.reference },
                    set: { studio.setReference(layer.id, reference: $0) }
                ))
            Menu("Colour tag") {
                Button("None") { studio.setTag(layer.id, tag: 0) }
                ForEach(1...6, id: \.self) { tag in
                    Button(tagName(tag)) { studio.setTag(layer.id, tag: tag) }
                }
            }
            Button("Move up") { studio.moveLayer(layer.id, steps: 1) }
            Button("Move down") { studio.moveLayer(layer.id, steps: -1) }
            Button("Rename…") {
                renameText = layer.name
                renameLayer = layer
            }
            Button("Delete", role: .destructive) {
                studio.selectLayer(layer.id)
                studio.removeLayer()
                refreshThumbnails()
            }
        }
    }

    private func icon(_ symbol: String, action: @escaping () -> Void) -> some View {
        Button(action: action) {
            Image(systemName: symbol)
                .font(.system(size: 16, weight: .medium))
                .foregroundStyle(Brand.paper.opacity(0.8))
                .frame(width: 36, height: 36)
        }
        .buttonStyle(.plain)
    }

    /// A collapsed group opens along the text direction; chevron.right is not a mirrored symbol.
    private func disclosureSymbol(_ layer: IOSLayer) -> String {
        if layer.expanded { return "chevron.down" }
        return layoutDirection == .rightToLeft ? "chevron.left" : "chevron.right"
    }

    private func tagColor(_ tag: Int) -> Color {
        switch tag {
        case 1: Color(red: 0.86, green: 0.24, blue: 0.22)
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

    private func refreshThumbnails() {
        var next: [Int: UIImage] = [:]
        for layer in studio.layers {
            if let image = studio.thumbnail(layer.id) {
                next[layer.id] = image
            }
        }
        thumbnails = next
    }
}
