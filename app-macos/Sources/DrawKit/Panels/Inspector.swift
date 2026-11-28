// The right-hand palette stack: colour, layers, figure.
//
// Clip Studio keeps these as docked palettes rather than a single inspector. The stack keeps that
// idea with honest cards: each one is a section a painter can scan without opening a tab.

import AppKit
import SwiftUI

struct SidePanel: View {
    let model: PaintModel
    @Binding var color: Color
    @Binding var recent: [Color]

    var body: some View {
        VStack(spacing: 10) {
            ColorSection(model: model, color: $color, recent: $recent)
            LayerSection(model: model, color: $color)
            if model.frameCount > 1 {
                FrameSection(model: model)
            }
            Spacer(minLength: 0)
        }
        .padding(10)
        .frame(width: 288)
        .background(.clear)
    }
}

struct PanelCard<Content: View>: View {
    let title: String
    let content: Content
    @State private var expanded: Bool
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @Environment(\.layoutDirection) private var layoutDirection

    init(
        _ title: String, startsExpanded: Bool = true,
        @ViewBuilder content: () -> Content
    ) {
        self.title = title
        self.content = content()
        _expanded = State(initialValue: startsExpanded)
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            Button {
                withAnimation(Motion.panel.resolving(reduceMotion: reduceMotion)) {
                    expanded.toggle()
                }
            } label: {
                HStack(spacing: 6) {
                    Text(title)
                        .font(.caption.weight(.semibold))
                        .foregroundStyle(Brand.ash)
                    Spacer(minLength: 0)
                    Image(systemName: "chevron.down")
                        .font(.system(size: 9, weight: .bold))
                        .foregroundStyle(Brand.muted)
                        // A collapsed disclosure points along the text direction: right in LTR,
                        // left in RTL. chevron.down is not a mirrored SF Symbol, so the angle is
                        // chosen here.
                        .rotationEffect(
                            .degrees(
                                expanded
                                    ? 0
                                    : (layoutDirection == .rightToLeft ? 90 : -90))
                        )
                        .frame(width: 22, height: 22)
                }
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .help(expanded ? "Collapse" : "Expand")
            if expanded {
                content
                    .transition(.opacity.combined(with: .move(edge: .top)))
            }
        }
        .padding(12)
        .frame(maxWidth: .infinity, alignment: .leading)
        .proofPlate()
    }
}

struct ColorSection: View {
    let model: PaintModel
    @Binding var color: Color
    @Binding var recent: [Color]
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var showColors = false

    private let swatches: [Color] = [
        .black,
        .white,
        Color(red: 0.86, green: 0.20, blue: 0.20),
        Color(red: 0.95, green: 0.62, blue: 0.20),
        Color(red: 0.95, green: 0.85, blue: 0.25),
        Color(red: 0.30, green: 0.70, blue: 0.35),
        Color(red: 0.25, green: 0.55, blue: 0.90),
        Color(red: 0.55, green: 0.35, blue: 0.80),
    ]

    var body: some View {
        PanelCard("Colour") {
            HStack(spacing: 10) {
                RoundedRectangle(cornerRadius: 7, style: .continuous)
                    .fill(color)
                    .frame(width: 46, height: 32)
                    .overlay(
                        RoundedRectangle(cornerRadius: 7, style: .continuous)
                            .stroke(Color.white.opacity(0.7), lineWidth: 1.5)
                    )
                    .shadow(color: .black.opacity(0.3), radius: 4, y: 2)
                VStack(alignment: .leading, spacing: 1) {
                    Text(hex)
                        .font(.callout.monospacedDigit())
                    Text(model.colorSpace.title)
                        .font(.caption2)
                        .foregroundStyle(.secondary)
                }
                Spacer()
                Button {
                    showColors = true
                } label: {
                    Text("Colours")
                        .font(.caption.weight(.medium))
                        .foregroundStyle(Brand.paper.opacity(0.8))
                        .padding(.horizontal, 10)
                        .frame(height: 26)
                        .background(
                            RoundedRectangle(cornerRadius: 8, style: .continuous)
                                .fill(Color.white.opacity(0.06))
                        )
                }
                .buttonStyle(.plain)
                .popover(isPresented: $showColors, arrowEdge: .trailing) {
                    ColorPanel(color: $color, recent: $recent, model: model)
                }
            }

            HStack(spacing: 6) {
                ForEach(Array(quick.enumerated()), id: \.offset) { _, swatch in
                    Circle()
                        .fill(swatch)
                        .frame(width: 26, height: 26)
                        .overlay(Circle().stroke(Brand.paper.opacity(0.7), lineWidth: 1.2))
                        .shadow(color: .black.opacity(0.25), radius: 2, y: 1)
                        .transition(.scale(scale: 0.6).combined(with: .opacity))
                        .contentShape(Circle())
                        .onTapGesture {
                            withAnimation(Motion.settle.resolving(reduceMotion: reduceMotion)) {
                                color = swatch
                            }
                        }
                }
            }
        }
    }

    /// Recent colours when there are any, the starting palette before that.
    private var quick: [Color] {
        recent.isEmpty ? swatches : Array(recent.prefix(8))
    }

    private var hex: String {
        guard let ns = NSColor(color).usingColorSpace(.deviceRGB) else { return "#000000" }
        return String(
            format: "#%02X%02X%02X",
            Int(ns.redComponent * 255), Int(ns.greenComponent * 255),
            Int(ns.blueComponent * 255))
    }
}

struct LayerSection: View {
    let model: PaintModel
    @Binding var color: Color
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var query = ""

    var body: some View {
        PanelCard("Layers") {
            HStack {
                Text("\(model.layers.count) layer\(model.layers.count == 1 ? "" : "s")")
                    .font(.caption)
                    .foregroundStyle(.secondary)
                Spacer()
                headerButton("plus", help: "Add a layer above the active one") {
                    model.addLayer()
                }
                headerButton("folder.badge.plus", help: "Add a group above the active item") {
                    model.addGroup()
                }
                headerButton(
                    "circle.lefthalf.filled",
                    help: "Add a fill layer with the current colour"
                ) {
                    model.addFillLayer(color)
                }
                headerButton("minus", help: "Remove the active layer or group") {
                    model.removeLayer()
                }
            }

            TextField("Search layers", text: $query)
                .textFieldStyle(.roundedBorder)
                .controlSize(.small)

            List {
                ForEach(
                    rows.filter { query.isEmpty || $0.name.localizedCaseInsensitiveContains(query) }
                ) { layer in
                    LayerRow(model: model, layer: layer)
                        .listRowInsets(EdgeInsets(top: 1, leading: 0, bottom: 1, trailing: 0))
                        .listRowSeparator(.hidden)
                        .listRowBackground(Color.clear)
                        .swipeActions(edge: .trailing, allowsFullSwipe: true) {
                            Button(role: .destructive) {
                                delete(layer)
                            } label: {
                                Label("Delete", systemImage: "trash")
                            }
                        }
                }
            }
            .listStyle(.plain)
            .scrollContentBackground(.hidden)
            .environment(\.defaultMinListRowHeight, 40)
            .frame(height: min(CGFloat(rows.count) * 42 + 8, 220))
            // Adding, removing or reordering a layer slides the rows and the list height instead
            // of snapping to the new arrangement.
            .animation(Motion.settle.resolving(reduceMotion: reduceMotion), value: model.layers)

            if let active = model.layers.first(where: { $0.id == model.activeLayer }) {
                HStack(spacing: 6) {
                    Text("Opacity").font(.caption).foregroundStyle(.secondary)
                    PenSlider(
                        value: Binding(
                            get: { active.opacity },
                            set: { model.setOpacity(index: active.id, $0) }
                        ),
                        range: 0...1,
                        width: 130
                    )
                    Text("\(Int(active.opacity * 100))%")
                        .font(.caption.monospacedDigit())
                        .frame(width: 34, alignment: .trailing)
                        .foregroundStyle(.secondary)
                }
                HStack(spacing: 6) {
                    Text("Blend").font(.caption).foregroundStyle(.secondary)
                    Spacer(minLength: 0)
                    Menu {
                        ForEach(BlendMode.allCases) { mode in
                            Button(mode.title) {
                                model.setBlend(index: active.id, mode)
                            }
                        }
                    } label: {
                        HStack(spacing: 5) {
                            Text(active.blend.title)
                                .font(.caption.weight(.medium))
                                .foregroundStyle(Brand.paper.opacity(0.85))
                            Image(systemName: "chevron.up.chevron.down")
                                .font(.system(size: 8, weight: .semibold))
                                .foregroundStyle(Brand.muted)
                        }
                        .padding(.horizontal, 9)
                        .frame(height: 26)
                        .background(
                            RoundedRectangle(cornerRadius: 8, style: .continuous)
                                .fill(Color.white.opacity(0.06))
                        )
                    }
                    .menuStyle(.borderlessButton)
                    .menuIndicator(.hidden)
                    .fixedSize()
                }
            }
        }
    }

    private func headerButton(
        _ symbol: String, help: String,
        action: @escaping () -> Void
    ) -> some View {
        Button {
            withAnimation(Motion.panel.resolving(reduceMotion: reduceMotion)) { action() }
        } label: {
            Image(systemName: symbol)
                .font(.system(size: 12, weight: .medium))
                .foregroundStyle(Brand.paper.opacity(0.72))
                .frame(width: 28, height: 28)
                .background(
                    RoundedRectangle(cornerRadius: 7, style: .continuous)
                        .fill(Color.white.opacity(0.05))
                )
        }
        .buttonStyle(.plain)
        .help(help)
    }

    private func delete(_ layer: LayerInfo) {
        if model.activeLayer != layer.id {
            model.selectLayer(layer.id)
        }
        withAnimation(Motion.panel.resolving(reduceMotion: reduceMotion)) {
            model.removeLayer()
        }
    }

    /// Top-down tree rows: a group header, then its children when expanded.
    private var rows: [LayerInfo] { model.paletteRows() }

}

struct FigureSection: View {
    @Bindable var figure: FigureModel
    let model: PaintModel

    var body: some View {
        PanelCard("3D figure") {
            HStack {
                Text(figure.available ? "Reference overlay" : "No GPU adapter")
                    .font(.caption)
                    .foregroundStyle(.secondary)
                Spacer()
                Toggle("", isOn: $figure.visible)
                    .labelsHidden()
                    .help("Show the reference figure")
            }

            if figure.available {
                Picker("Body", selection: $figure.preset) {
                    ForEach(Array(FigureModel.presetNames.enumerated()), id: \.offset) {
                        index, name in
                        Text(name).tag(index)
                    }
                }
                .pickerStyle(.menu)
                .font(.caption)

                HStack {
                    Menu("Pose") {
                        ForEach(Array(FigureModel.poseNames.enumerated()), id: \.offset) {
                            index, name in
                            Button(name) { figure.applyPose(index) }
                        }
                    }
                    Button("Mirror") { figure.mirror() }
                    Button("Reset") { figure.reset() }
                }
                .controlSize(.small)

                HStack(spacing: 6) {
                    Text("Opacity").font(.caption).foregroundStyle(.secondary)
                    Slider(value: $figure.opacity, in: 0...1)
                }

                HStack {
                    Toggle("Toon shading", isOn: $figure.toon)
                    Toggle("Outline", isOn: $figure.outline)
                }
                .font(.caption)
                .toggleStyle(.checkbox)

                DisclosureGroup("Lighting") {
                    lightRow("Key", value: $figure.keyIntensity, in: 0...2)
                    lightRow("Key angle", value: $figure.keyAzimuth, in: -180...180)
                    lightRow("Key height", value: $figure.keyElevation, in: 0...90)
                    lightRow("Key warmth", value: $figure.keyWarmth, in: -1...1)
                    lightRow("Fill", value: $figure.fillIntensity, in: 0...1.5)
                    lightRow("Fill angle", value: $figure.fillAzimuth, in: -180...180)
                    lightRow("Fill warmth", value: $figure.fillWarmth, in: -1...1)
                    lightRow("Rim", value: $figure.rimIntensity, in: 0...1.5)
                    lightRow("Rim angle", value: $figure.rimAzimuth, in: -180...180)
                    lightRow("Rim warmth", value: $figure.rimWarmth, in: -1...1)
                    lightRow("Ambient", value: $figure.ambient, in: 0...1)
                    HStack {
                        Button("Estimate light (preview)") { figure.estimateLighting(from: model) }
                        Spacer()
                        Button("Reset light") { figure.resetLighting() }
                    }
                    .controlSize(.small)
                }
                .font(.caption)

                Button("Bake into layer") { figure.bake(into: model) }
                    .controlSize(.small)
                    .disabled(figure.image == nil)
                    .help("Draw the reference into a new layer at full opacity")

                Text("Hands/feet use IK; elbows/knees aim; drag elsewhere to orbit.")
                    .font(.caption2)
                    .foregroundStyle(.secondary)
            }
        }
    }

    private func lightRow(
        _ title: String, value: Binding<Double>,
        in range: ClosedRange<Double>
    ) -> some View {
        HStack(spacing: 6) {
            Text(title)
                .font(.caption2)
                .foregroundStyle(.secondary)
                .frame(width: 62, alignment: .leading)
            Slider(value: value, in: range)
            Text(
                title.contains("angle")
                    ? "\(Int(value.wrappedValue))°"
                    : String(format: "%.2f", value.wrappedValue)
            )
            .font(.caption2.monospacedDigit())
            .foregroundStyle(.secondary)
            .frame(width: 34, alignment: .trailing)
        }
    }
}
