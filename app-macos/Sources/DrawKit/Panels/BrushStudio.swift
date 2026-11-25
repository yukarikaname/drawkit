// Brush Studio: the full set of parameters the current brush actually uses, plus a live preview
// painted through the same engine. Changing a slider edits the current brush, not a document.

import SwiftUI

struct BrushStudio: View {
    @Bindable var model: PaintModel
    @Binding var brush: BrushKind
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    private struct Setting: Identifiable {
        let id: Int
        let title: String
        let range: ClosedRange<Double>
        let format: String
        let help: String

        var display: (Double) -> String {
            { value in String(format: format, value) }
        }
    }

    private let groups: [(String, [Setting])] = [
        ("SHAPE", [
            Setting(id: 0, title: "Hardness", range: 0...1, format: "%.2f",
                    help: "0 is a soft falloff, 1 is a solid edge"),
            Setting(id: 4, title: "Spacing", range: 0.02...1.5, format: "%.2f",
                    help: "Distance between stamps as a fraction of the radius"),
            Setting(id: 5, title: "Jitter", range: 0...1.5, format: "%.2f",
                    help: "Random offset of each stamp"),
            Setting(id: 6, title: "Angle", range: 0...180, format: "%.0f°",
                    help: "Rotation of an elliptical stamp"),
            Setting(id: 7, title: "Roundness", range: 0.05...1, format: "%.2f",
                    help: "Short axis of the stamp; 1 is a circle"),
        ]),
        ("INK", [
            Setting(id: 1, title: "Flow", range: 0.01...1, format: "%.2f",
                    help: "Opacity laid down by one pass"),
            Setting(id: 8, title: "Grain", range: 0...1, format: "%.2f",
                    help: "Per-pixel coverage variation"),
        ]),
        ("TEXTURE", [
            Setting(id: 9, title: "Paper", range: 0...1, format: "%.2f",
                    help: "Stable paper grain sampled in canvas space"),
            Setting(id: 10, title: "Tip", range: 0...1, format: "%.2f",
                    help: "Texture inside each dab"),
            Setting(id: 12, title: "Dual", range: 0...1, format: "%.2f",
                    help: "Second tip offset perpendicular to the stroke"),
            Setting(id: 13, title: "Dual size", range: 0.1...1.5, format: "%.2f",
                    help: "Size of the second tip"),
        ]),
        ("MIX", [
            Setting(id: 11, title: "Wet", range: 0...1, format: "%.2f",
                    help: "Pick up colour already under the stroke"),
        ]),
        ("PRESSURE", [
            Setting(id: 2, title: "Size", range: 0...1, format: "%.2f",
                    help: "Radius at zero pressure, as a fraction of the nominal radius"),
            Setting(id: 3, title: "Opacity", range: 0...1, format: "%.2f",
                    help: "Opacity lost at zero pressure"),
        ]),
    ]

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            ScrollView(.horizontal, showsIndicators: false) {
                HStack(spacing: 5) {
                    ForEach(BrushKind.allCases) { kind in
                        Button { brush = kind } label: {
                            Group {
                                if kind.rawValue < model.brushSlotPreviews.count,
                                   let preview = model.brushSlotPreviews[kind.rawValue] {
                                    Image(decorative: preview, scale: 1)
                                        .resizable()
                                        .aspectRatio(168.0 / 40.0, contentMode: .fit)
                                } else {
                                    Image(systemName: kind.symbol)
                                }
                            }
                            .frame(width: 76, height: 28)
                            .padding(3)
                            .background(
                                RoundedRectangle(cornerRadius: 7, style: .continuous)
                                    .fill(brush == kind ? Brand.vermilion.opacity(0.85)
                                          : Color.white.opacity(0.04))
                            )
                        }
                        .buttonStyle(.plain)
                        .help(kind.title)
                    }
                }
            }
            if let preview = model.brushPreview {
                Image(decorative: preview, scale: 1)
                    .resizable()
                    .aspectRatio(240.0 / 56.0, contentMode: .fit)
                    .clipShape(RoundedRectangle(cornerRadius: 8, style: .continuous))
                    .overlay(RoundedRectangle(cornerRadius: 8, style: .continuous)
                        .stroke(.white.opacity(0.10), lineWidth: 1))
            }
            ForEach(groups, id: \.0) { group in
                Text(group.0)
                    .font(.caption2.weight(.semibold))
                    .tracking(1)
                    .foregroundStyle(Brand.muted)
                ForEach(group.1) { setting in
                    HStack(spacing: 8) {
                        Text(setting.title)
                            .font(.caption)
                            .foregroundStyle(Brand.paper.opacity(0.78))
                            .frame(width: 58, alignment: .leading)
                            .help(setting.help)
                        PenSlider(value: binding(setting.id), range: setting.range, width: 150)
                        Text(setting.display(model.brushParameters[setting.id]))
                            .font(.caption.monospacedDigit())
                            .foregroundStyle(Brand.muted)
                            .frame(width: 42, alignment: .trailing)
                    }
                }
            }
            HStack {
                Button("Reset to preset") {
                    withAnimation(Motion.panel.resolving(reduceMotion: reduceMotion)) {
                        model.resetBrush()
                    }
                }
                .controlSize(.small)
                Spacer()
                Text("Applies to the current brush")
                    .font(.caption2)
                    .foregroundStyle(Brand.muted)
            }
        }
        .padding(14)
        .frame(width: 330)
        .onAppear { model.refreshBrush() }
    }

    private func binding(_ id: Int) -> Binding<Double> {
        Binding(
            get: { model.brushParameters[id] },
            set: { model.setBrushParameter(id, $0) }
        )
    }
}
