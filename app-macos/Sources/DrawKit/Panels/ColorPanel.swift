// The colour panel. It has the three views a painter expects - a grid of swatches, the spectrum
// wheel and sliders - and nothing else. The default is the grid: most strokes reuse a colour, and a
// grid answers that faster than a wheel.
//
// The panel is a macOS sheet, so it gets its own window chrome. Tabs use the system segmented
// control and sliders use the system slider; only the grid and the spectrum are drawn here.

import AppKit
import SwiftUI
import UniformTypeIdentifiers

struct ColorPanel: View {
    @Binding var color: Color
    @Binding var recent: [Color]
    let model: PaintModel

    @Environment(\.dismiss) private var dismiss
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var mode: Mode = .grid
    @State private var palette: [Color] = []

    enum Mode: String, CaseIterable, Identifiable {
        case grid = "Grid"
        case spectrum = "Spectrum"
        case sliders = "Sliders"

        var id: Self { self }
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            header
            Picker("View", selection: $mode) {
                ForEach(Mode.allCases) { mode in
                    Text(mode.rawValue).tag(mode)
                }
            }
            .pickerStyle(.segmented)
            .labelsHidden()

            Group {
                switch mode {
                case .grid:
                    ColorGrid(color: $color)
                        .frame(maxHeight: .infinity)
                case .spectrum:
                    ColorWheelView(color: $color, colorSpace: model.colorSpace)
                        .frame(maxWidth: 300)
                        .frame(maxWidth: .infinity)
                case .sliders:
                    ColorSliders(color: $color)
                }
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
            .transition(.opacity.combined(with: .scale(scale: 0.98)))
            .animation(Motion.select.resolving(reduceMotion: reduceMotion), value: mode)

            if !palette.isEmpty {
                paletteStrip
            }

            footer
        }
        .padding(18)
        .frame(width: 340, height: 520)
        .onExitCommand { dismiss() }
    }

    private var header: some View {
        HStack(spacing: 12) {
            BrandMark(size: 26)
            Text("Colours")
                .font(.system(size: 28, weight: .bold, design: .rounded))
            RoundedRectangle(cornerRadius: 7)
                .fill(color)
                .frame(width: 54, height: 28)
                .overlay(
                    RoundedRectangle(cornerRadius: 7)
                        .stroke(Color.primary.opacity(0.18), lineWidth: 1))
            Spacer()
            Button("Done") { dismiss() }
                .buttonStyle(.borderedProminent)
                .controlSize(.large)
                .keyboardShortcut(.defaultAction)
        }
    }

    private var paletteStrip: some View {
        ScrollView(.horizontal, showsIndicators: false) {
            HStack(spacing: 6) {
                ForEach(Array(palette.enumerated()), id: \.offset) { _, swatch in
                    Circle()
                        .fill(swatch)
                        .frame(width: 24, height: 24)
                        .overlay(Circle().stroke(Color.primary.opacity(0.18), lineWidth: 1))
                        .onTapGesture {
                            withAnimation(Motion.settle.resolving(reduceMotion: reduceMotion)) {
                                color = swatch
                            }
                        }
                }
            }
            .padding(.vertical, 2)
        }
        .frame(height: 30)
        .help("Imported palette")
    }

    private var footer: some View {
        HStack(spacing: 7) {
            Circle()
                .fill(color)
                .frame(width: 30, height: 30)
                .overlay(Circle().stroke(Color.primary.opacity(0.2), lineWidth: 1))

            Button {
                remember(color)
            } label: {
                Image(systemName: "plus")
                    .font(.system(size: 12, weight: .semibold))
                    .frame(width: 28, height: 28)
                    .background(Circle().fill(Color(nsColor: .controlBackgroundColor)))
                    .overlay(Circle().stroke(Color.primary.opacity(0.12), lineWidth: 1))
            }
            .buttonStyle(.plain)
            .help("Keep this colour")

            ForEach(Array(recent.prefix(7).enumerated()), id: \.offset) { _, swatch in
                Circle()
                    .fill(swatch)
                    .frame(width: 24, height: 24)
                    .overlay(Circle().stroke(Color.primary.opacity(0.18), lineWidth: 1))
                    .transition(.scale(scale: 0.6).combined(with: .opacity))
                    .onTapGesture {
                        withAnimation(Motion.settle.resolving(reduceMotion: reduceMotion)) {
                            color = swatch
                        }
                    }
            }

            Spacer(minLength: 0)

            Menu {
                Button("Import palette…") { importPalette() }
                Divider()
                Button("Export GIMP palette…") { exportPalette(format: 0, extension: "gpl") }
                Button("Export Adobe palette…") { exportPalette(format: 1, extension: "ase") }
            } label: {
                Image(systemName: "swatchpalette")
                    .font(.system(size: 13))
                    .frame(width: 28, height: 28)
                    .background(Circle().fill(Color(nsColor: .controlBackgroundColor)))
                    .overlay(Circle().stroke(Color.primary.opacity(0.12), lineWidth: 1))
            }
            .menuStyle(.borderlessButton)
            .menuIndicator(.hidden)
            .fixedSize()
            .help("Palette files")

            Button {
                sampleArtwork()
            } label: {
                Image(systemName: "sparkles")
                    .font(.system(size: 13))
                    .frame(width: 28, height: 28)
                    .background(Circle().fill(Color(nsColor: .controlBackgroundColor)))
                    .overlay(Circle().stroke(Color.primary.opacity(0.12), lineWidth: 1))
            }
            .buttonStyle(.plain)
            .help("Sample colours from the artwork")
        }
        .frame(height: 32)
    }

    private func remember(_ colour: Color) {
        guard !recent.contains(colour) else { return }
        withAnimation(Motion.panel.resolving(reduceMotion: reduceMotion)) {
            recent.insert(colour, at: 0)
            recent = Array(recent.prefix(10))
        }
    }

    private func importPalette() {
        let panel = NSOpenPanel()
        panel.allowedContentTypes = ["gpl", "ase"].compactMap { UTType(filenameExtension: $0) }
        panel.allowsMultipleSelection = false
        guard panel.runModal() == .OK, let url = panel.url else { return }
        var values = [Float](repeating: 0, count: 256 * 3)
        let count = values.withUnsafeMutableBufferPointer { buffer -> Int32 in
            guard let base = buffer.baseAddress else { return 0 }
            return url.path.withCString {
                dk_read_palette($0, base, UInt32(buffer.count / 3))
            }
        }
        guard count > 0 else {
            NSSound.beep()
            return
        }
        var imported: [Color] = []
        imported.reserveCapacity(Int(count))
        for index in 0..<Int(count) {
            let red = Double(values[index * 3])
            let green = Double(values[index * 3 + 1])
            let blue = Double(values[index * 3 + 2])
            imported.append(Color(.sRGB, red: red, green: green, blue: blue, opacity: 1))
        }
        palette = imported
        if let first = palette.first {
            color = first
            remember(first)
        }
    }

    private func exportPalette(format: Int32, extension pathExtension: String) {
        let colors = palette.isEmpty ? recent : palette
        guard !colors.isEmpty else { return }
        let panel = NSSavePanel()
        panel.allowedContentTypes = [UTType(filenameExtension: pathExtension) ?? .data]
        panel.nameFieldStringValue = "drawkit.\(pathExtension)"
        guard panel.runModal() == .OK, let url = panel.url else { return }
        var values: [Float] = []
        values.reserveCapacity(colors.count * 3)
        for entry in colors {
            guard let ns = NSColor(entry).usingColorSpace(.sRGB) else { continue }
            values.append(Float(ns.redComponent))
            values.append(Float(ns.greenComponent))
            values.append(Float(ns.blueComponent))
        }
        guard !values.isEmpty else { return }
        let saved = values.withUnsafeBufferPointer { buffer -> Int32 in
            guard let base = buffer.baseAddress else { return 0 }
            return url.path.withCString {
                dk_write_palette($0, base, UInt32(buffer.count / 3), format)
            }
        }
        if saved == 0 { NSSound.beep() }
    }

    private func sampleArtwork() {
        let sampled = model.palette(count: 6)
        guard let first = sampled.first else { return }
        withAnimation(Motion.panel.resolving(reduceMotion: reduceMotion)) {
            color = first
            for swatch in sampled.reversed() where !recent.contains(swatch) {
                recent.insert(swatch, at: 0)
            }
            recent = Array(recent.prefix(10))
        }
    }
}

/// A hue-by-lightness grid under a grey ramp, tapped or dragged to pick.
///
/// The grid is one `Canvas`: ninety-six swatches are pixels, not views, and a drag across it feels
/// like moving a pen through colour rather than pressing ninety-six buttons.
private struct ColorGrid: View {
    @Binding var color: Color

    private let columns = 12
    private let rows = 7

    var body: some View {
        GeometryReader { geometry in
            Canvas { context, size in
                let cell = cellSize(in: size)
                for column in 0..<columns {
                    let grey = 1 - Double(column) / Double(columns - 1)
                    context.fill(path(column, 0, cell), with: .color(Color(white: grey)))
                }
                for row in 0..<rows {
                    for column in 0..<columns {
                        context.fill(
                            path(column, row + 1, cell), with: .color(cellColor(column, row)))
                    }
                }
                if let selected = selectedCell() {
                    let rect = path(selected.0, selected.1, cell).boundingRect.insetBy(dx: 2, dy: 2)
                    context.stroke(Path(rect), with: .color(.white), lineWidth: 2)
                }
            }
            .contentShape(Rectangle())
            .gesture(
                DragGesture(minimumDistance: 0)
                    .onChanged { value in pick(value.location, in: geometry.size) })
        }
        .clipShape(RoundedRectangle(cornerRadius: 8))
    }

    private func cellSize(in size: CGSize) -> CGSize {
        CGSize(width: size.width / CGFloat(columns), height: size.height / CGFloat(rows + 1))
    }

    private func path(_ column: Int, _ row: Int, _ cell: CGSize) -> Path {
        Path(
            CGRect(
                x: CGFloat(column) * cell.width, y: CGFloat(row) * cell.height,
                width: cell.width, height: cell.height))
    }

    /// Columns are hue; rows run from a deep value to a pastel, so both ends of a hue are reachable
    /// without moving a slider.
    private func cellColor(_ column: Int, _ row: Int) -> Color {
        let hue = (Double(column) + 0.5) / Double(columns)
        let lightness = 0.16 + Double(row) / Double(rows - 1) * 0.74
        let (r, g, b) = hslToRgb(hue, 1, lightness)
        return Color(.sRGB, red: r, green: g, blue: b, opacity: 1)
    }

    private func pick(_ point: CGPoint, in size: CGSize) {
        guard size.width > 0, size.height > 0 else { return }
        let cell = cellSize(in: size)
        let column = min(max(Int(point.x / cell.width), 0), columns - 1)
        let row = min(max(Int(point.y / cell.height), 0), rows)
        if row == 0 {
            color = Color(white: 1 - Double(column) / Double(columns - 1))
        } else {
            color = cellColor(column, row - 1)
        }
    }

    /// The cell the current colour sits nearest in hue and lightness. Always returns a cell, so the
    /// outline keeps following colours chosen in the wheel or the sliders.
    private func selectedCell() -> (Int, Int)? {
        guard let ns = NSColor(color).usingColorSpace(.deviceRGB) else { return nil }
        var hue: CGFloat = 0
        var saturation: CGFloat = 0
        var brightness: CGFloat = 0
        var alpha: CGFloat = 1
        ns.getHue(&hue, saturation: &saturation, brightness: &brightness, alpha: &alpha)
        if saturation < 0.10 {
            let column = Int(((1 - Double(brightness)) * Double(columns - 1)).rounded())
            return (min(max(column, 0), columns - 1), 0)
        }
        let column = min(Int(Double(hue) * Double(columns)), columns - 1)
        let lightness = Double(brightness) * (1 - Double(saturation) / 2)
        let row = ((lightness - 0.16) / 0.74 * Double(rows - 1)).rounded()
        return (column, min(max(Int(row), 0), rows - 1) + 1)
    }
}

/// Hue, saturation, brightness and opacity on system sliders.
private struct ColorSliders: View {
    @Binding var color: Color

    @State private var hue: CGFloat = 0
    @State private var saturation: CGFloat = 0
    @State private var brightness: CGFloat = 0
    @State private var opacity: CGFloat = 1
    @State private var updating = false

    var body: some View {
        VStack(spacing: 14) {
            row("Hue", hueBinding, value: "\(Int(hue * 360))°")
            row("Saturation", saturationBinding, value: "\(Int(saturation * 100))%")
            row("Brightness", brightnessBinding, value: "\(Int(brightness * 100))%")
            row("Opacity", opacityBinding, value: "\(Int(opacity * 100))%")
            Spacer(minLength: 0)
        }
        .onAppear(perform: syncFromColor)
        .onChange(of: color) { _, _ in
            guard !updating else { return }
            syncFromColor()
        }
    }

    private func row(_ title: String, _ value: Binding<CGFloat>, value text: String) -> some View {
        HStack(spacing: 10) {
            Text(title)
                .font(.callout)
                .foregroundStyle(.secondary)
                .frame(width: 76, alignment: .leading)
            PenSlider(value: pen(value), range: 0...1, width: 170)
            Text(text)
                .font(.callout.monospacedDigit())
                .frame(width: 44, alignment: .trailing)
        }
    }

    private func pen(_ value: Binding<CGFloat>) -> Binding<Double> {
        Binding(
            get: { Double(value.wrappedValue) },
            set: { value.wrappedValue = CGFloat($0) }
        )
    }

    private var hueBinding: Binding<CGFloat> {
        Binding(
            get: { hue },
            set: {
                hue = $0
                apply()
            })
    }

    private var saturationBinding: Binding<CGFloat> {
        Binding(
            get: { saturation },
            set: {
                saturation = $0
                apply()
            })
    }

    private var brightnessBinding: Binding<CGFloat> {
        Binding(
            get: { brightness },
            set: {
                brightness = $0
                apply()
            })
    }

    private var opacityBinding: Binding<CGFloat> {
        Binding(
            get: { opacity },
            set: {
                opacity = $0
                apply()
            })
    }

    private func apply() {
        updating = true
        color = Color(
            hue: Double(hue), saturation: Double(saturation),
            brightness: Double(brightness), opacity: Double(opacity))
        updating = false
    }

    private func syncFromColor() {
        guard let ns = NSColor(color).usingColorSpace(.deviceRGB) else { return }
        var h: CGFloat = 0
        var s: CGFloat = 0
        var b: CGFloat = 0
        var a: CGFloat = 1
        ns.getHue(&h, saturation: &s, brightness: &b, alpha: &a)
        hue = h
        saturation = s
        brightness = b
        opacity = a
    }
}

/// HSL, the model the grid is built from. Hue is 0...1, saturation and lightness 0...1.
func hslToRgb(_ hue: Double, _ saturation: Double, _ lightness: Double) -> (Double, Double, Double)
{
    let c = (1 - abs(2 * lightness - 1)) * saturation
    let h = (hue - floor(hue)) * 6
    let x = c * (1 - abs(h.truncatingRemainder(dividingBy: 2) - 1))
    let (r, g, b): (Double, Double, Double)
    switch Int(h) {
    case 0: (r, g, b) = (c, x, 0)
    case 1: (r, g, b) = (x, c, 0)
    case 2: (r, g, b) = (0, c, x)
    case 3: (r, g, b) = (0, x, c)
    case 4: (r, g, b) = (x, 0, c)
    default: (r, g, b) = (c, 0, x)
    }
    let m = lightness - c / 2
    return (r + m, g + m, b + m)
}

/// The utility window around the panel. A painter can keep it open, paint, and sample without
/// dismissing a modal sheet first.
