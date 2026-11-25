// The tool property bar: the settings that belong to the selected tool, directly above the canvas.
//
// CSP changes this row when the tool changes, and so does this. A setting that does not apply to
// the current tool is not shown disabled - it is not shown at all, which keeps the row readable.

import SwiftUI

struct ToolOptionsBar: View {
    let tool: Tool
    let model: PaintModel
    let shortcuts: ShortcutStore
    let onImportBrush: () -> Void
    @Binding var size: Double
    @Binding var smudgeStrength: Double
    @Binding var stabilization: Double
    @Binding var tolerance: Double
    @Binding var fillClose: Double
    @Binding var fillExpand: Double
    @Binding var fillFeather: Double
    @Binding var fillSnap: Bool
    @Binding var strokeDedupe: Bool
    @Binding var prediction: Bool
    @Binding var autoShape: Bool
    @Binding var quickShape: Bool
    @Binding var mirrorEnabled: Bool
    @Binding var mirrorAngle: Double
    @Binding var mirrorBoth: Bool
    let hdr: Bool
    @Binding var glow: Double
    @Binding var exposure: Double
    @Binding var brush: BrushKind
    @State private var showBrushStudio = false
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    var body: some View {
        HStack(spacing: 14) {
            switch tool {
            case .brush, .eraser:
                option("Size", value: "\(Int(size)) px") {
                    HStack(spacing: 4) {
                        PenSlider(value: $size, range: 1...48, width: 130)
                        PenStepper(value: $size, range: 1...48)
                    }
                }
                option("Radius in", value: model.brushScale.title) {
                    Picker("", selection: Binding(
                        get: { model.brushScale },
                        set: { model.brushScale = $0 })) {
                            Text("Canvas").tag(BrushScale.canvas)
                            Text("Screen").tag(BrushScale.screen)
                        }
                        .pickerStyle(.segmented)
                        .labelsHidden()
                        .frame(width: 150)
                }
                option("Brush", value: brush.title) {
                    HStack(spacing: 4) {
                        ForEach(BrushKind.allCases) { kind in
                            BrushToken(kind: kind, selected: brush == kind,
                                       reduceMotion: reduceMotion) {
                                withAnimation(Motion.select.resolving(reduceMotion: reduceMotion)) {
                                    brush = kind
                                }
                            }
                        }
                        Button(action: onImportBrush) {
                            Image(systemName: "plus")
                                .font(.system(size: 11, weight: .semibold))
                                .foregroundStyle(Brand.paper.opacity(0.72))
                                .frame(width: 27, height: 27)
                                .background(
                                    RoundedRectangle(cornerRadius: 8, style: .continuous)
                                        .fill(Color.white.opacity(0.035))
                                )
                        }
                        .buttonStyle(SpringPressStyle(reduceMotion: reduceMotion))
                        .help("Import a MyPaint brush (.myb)")
                        Button { showBrushStudio = true } label: {
                            Group {
                                if let preview = model.brushPreview {
                                    Image(decorative: preview, scale: 1)
                                        .resizable()
                                        .aspectRatio(240.0 / 56.0, contentMode: .fit)
                                } else {
                                    Image(systemName: "slider.horizontal.3")
                                        .font(.system(size: 11, weight: .semibold))
                                }
                            }
                            .foregroundStyle(Brand.paper.opacity(0.78))
                            .frame(width: 46, height: 27)
                            .background(
                                RoundedRectangle(cornerRadius: 8, style: .continuous)
                                    .fill(Color.white.opacity(0.035))
                            )
                        }
                        .buttonStyle(SpringPressStyle(reduceMotion: reduceMotion))
                        .popover(isPresented: $showBrushStudio, arrowEdge: .bottom) {
                            BrushStudio(model: model, brush: $brush)
                        }
                        .help("Brush Studio: the parameters behind this preset")
                    }
                }
                option("Smooth", value: String(format: "%.1f", stabilization)) {
                    PenSlider(value: $stabilization, range: 0...10, width: 100)
                }
                Menu {
                    Toggle("No double ink", isOn: $strokeDedupe)
                    Toggle("Predict the next frame", isOn: $prediction)
                    Toggle("Auto shape (tiny model)", isOn: $autoShape)
                    Toggle("Hold to snap shape", isOn: $quickShape)
                } label: {
                    Label("Assists", systemImage: "slider.horizontal.3")
                        .font(.caption.weight(.medium))
                        .foregroundStyle(Brand.paper.opacity(0.82))
                        .padding(.horizontal, 9)
                        .frame(height: 34)
                        .background(
                            RoundedRectangle(cornerRadius: 9, style: .continuous)
                                .fill(Color.white.opacity(0.05))
                        )
                }
                .menuStyle(.borderlessButton)
                .fixedSize()
                .help("Input assists: duplicate samples and one-frame prediction")
                option("Mirror", value: "") {
                    HStack(spacing: 7) {
                        PenToggle(isOn: $mirrorEnabled, help: "Mirror the stroke")
                        if mirrorEnabled {
                            PenSlider(value: $mirrorAngle, range: 0...180, step: 1, width: 100)
                            Text("\(Int(mirrorAngle.rounded()))°")
                                .font(.caption.monospacedDigit())
                                .foregroundStyle(.secondary)
                                .frame(width: 30, alignment: .leading)
                            PenCheck(title: "Both", isOn: $mirrorBoth)
                        }
                    }
                }
                if hdr {
                    option("Glow", value: String(format: "+%.1f EV", glow)) {
                        PenSlider(value: $glow, range: 0...4, width: 100)
                    }
                    option("Exposure", value: String(format: "%+.1f EV", exposure)) {
                        PenSlider(value: $exposure, range: -4...4, width: 100)
                    }
                }
            case .smudge:
                option("Size", value: "\(Int(size)) px") {
                    HStack(spacing: 4) {
                        PenSlider(value: $size, range: 1...48, width: 130)
                        PenStepper(value: $size, range: 1...48)
                    }
                }
                option("Strength", value: "\(Int(smudgeStrength * 100))%") {
                    Slider(value: $smudgeStrength, in: 0.05...1)
                        .frame(width: 90)
                }
            case .eyedropper:
                Label("Click the canvas to sample a colour; drag to keep sampling.",
                      systemImage: "eyedropper")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            case .fill:
                option("Tolerance", value: "\(Int(tolerance * 100))%") {
                    PenSlider(value: $tolerance, range: 0.02...0.6, width: 120)
                }
                option("Gap", value: "\(Int(fillClose))") {
                    PenSlider(value: $fillClose, range: 0...4, step: 1, width: 90)
                }
                option("Grow", value: "\(Int(fillExpand))") {
                    PenSlider(value: $fillExpand, range: 0...8, step: 1, width: 90)
                }
                option("Soft", value: "\(Int(fillFeather))") {
                    PenSlider(value: $fillFeather, range: 0...12, step: 1, width: 90)
                }
                PenCheck(title: "Snap colour", isOn: $fillSnap)
                    .help("A refill close to the colour already there keeps that colour")
                option("Smart", value: model.fillSmart ? "On" : "Off") {
                    PenToggle(
                        isOn: Binding(get: { model.fillSmart },
                                      set: { model.fillSmart = $0 }),
                        help: "Use the local edge model to close a break in the line")
                }
            case .select:
                Label("Drag a rectangle. Edit menu: Select All, Deselect, Extract, Clear.",
                      systemImage: "rectangle.dashed")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            case .figure:
                Label("Drag a hand or foot for IK, an elbow or knee to aim, elsewhere to orbit.",
                      systemImage: "hand.draw")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
            Spacer(minLength: 0)
        }
        .tint(Brand.vermilion)
        .padding(.horizontal, 14)
        .frame(minHeight: 54)
        .background(
            RoundedRectangle(cornerRadius: 14, style: .continuous)
                .fill(Brand.carbon.opacity(0.5))
                .glassEffect(.regular, in: .rect(cornerRadius: 14))
                .overlay(
                    RoundedRectangle(cornerRadius: 14, style: .continuous)
                        .stroke(.white.opacity(0.10), lineWidth: 1)
                )
        )
        .shadow(color: .black.opacity(0.25), radius: 14, y: 6)
        .animation(Motion.select.resolving(reduceMotion: reduceMotion), value: tool)
        .animation(Motion.select.resolving(reduceMotion: reduceMotion), value: mirrorEnabled)
        .animation(Motion.panel.resolving(reduceMotion: reduceMotion), value: hdr)
    }

    private func option<Content: View>(
        _ name: String,
        value: String,
        @ViewBuilder content: () -> Content
    ) -> some View {
        HStack(spacing: 6) {
            Text(name)
                .font(.caption)
                .foregroundStyle(.secondary)
                .lineLimit(1)
                .fixedSize(horizontal: true, vertical: false)
            content()
            Text(value)
                .font(.caption.monospacedDigit())
                .foregroundStyle(.secondary)
                .lineLimit(1)
                .fixedSize(horizontal: true, vertical: false)
                .frame(minWidth: 44, alignment: .leading)
        }
    }
}


/// A pen in the tray: an object with a selected state, not a system menu.
private struct BrushToken: View {
    let kind: BrushKind
    let selected: Bool
    let reduceMotion: Bool
    let action: () -> Void

    @State private var hovering = false

    var body: some View {
        Button(action: action) {
            Image(systemName: kind.symbol)
                .font(.system(size: 12, weight: .medium))
                .foregroundStyle(selected ? Color.white : Brand.paper.opacity(hovering ? 0.95 : 0.68))
                .frame(width: 27, height: 27)
                .background(
                    RoundedRectangle(cornerRadius: 8, style: .continuous)
                        .fill(selected ? Brand.vermilion
                              : Color.white.opacity(hovering ? 0.09 : 0.035))
                )
                .overlay(
                    RoundedRectangle(cornerRadius: 8, style: .continuous)
                        .stroke(selected ? .white.opacity(0.22) : .white.opacity(0.05),
                                lineWidth: 1)
                )
                .shadow(color: selected ? Brand.vermilion.opacity(0.3) : .clear, radius: 5, y: 2)
        }
        .buttonStyle(SpringPressStyle(reduceMotion: reduceMotion))
        .help(kind.title)
        .onHover { inside in
            withAnimation(.easeOut(duration: 0.12)) { hovering = inside }
        }
    }
}
