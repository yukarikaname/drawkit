// The layer sheet: thumbnails, visibility, selection and the active layer's opacity.

import SwiftUI
import UIKit

struct LayersSheet: View {
    @Bindable var studio: Studio
    @Environment(\.dismiss) private var dismiss
    @State private var thumbnails: [Int: UIImage] = [:]
    @State private var query = ""

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

            ScrollView {
                LazyVStack(spacing: 4) {
                    ForEach(studio.layerRows().filter {
                        query.isEmpty || $0.name.localizedCaseInsensitiveContains(query)
                    }) { layer in
                        row(layer)
                    }
                }
            }
            .frame(maxHeight: 380)

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
                .tint(Brand.vermilion)
            }
        }
        .padding(20)
        .background(Brand.carbon)
        .preferredColorScheme(.dark)
        .presentationDetents([.medium, .large])
        .presentationDragIndicator(.visible)
        .onAppear(perform: refreshThumbnails)
    }

    private func row(_ layer: IOSLayer) -> some View {
        HStack(spacing: 10) {
            if layer.isGroup {
                Button {
                    studio.setExpanded(layer.id, !layer.expanded)
                } label: {
                    Image(systemName: layer.expanded ? "chevron.down" : "chevron.right")
                        .font(.system(size: 11, weight: .bold))
                        .foregroundStyle(Brand.muted)
                        .frame(width: 14)
                }
                .buttonStyle(.plain)
                Image(systemName: "folder.fill")
                    .font(.system(size: 20))
                    .foregroundStyle(Brand.paper.opacity(0.7))
                    .frame(width: 42, height: 42)
                    .background(Color.white.opacity(0.05),
                                in: RoundedRectangle(cornerRadius: 7))
            } else if layer.isFill {
                Color.clear.frame(width: 14)
                Image(systemName: "square.fill")
                    .font(.system(size: 20))
                    .foregroundStyle(Brand.paper.opacity(0.7))
                    .frame(width: 42, height: 42)
                    .background(Color.white.opacity(0.05),
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
                Text(layer.name)
                    .font(.system(size: 14, weight: layer.id == studio.activeLayer ? .semibold : .regular))
                    .foregroundStyle(Brand.paper)
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
                .fill(layer.id == studio.activeLayer
                      ? Brand.vermilion.opacity(0.18) : Color.clear)
        )
        .padding(.leading, CGFloat(layer.depth) * 16)
        .contentShape(Rectangle())
        .onTapGesture { studio.selectLayer(layer.id) }
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
