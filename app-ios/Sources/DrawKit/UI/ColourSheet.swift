// The iPad colour sheet: the same grid and HSV sliders as the other shells, in a native sheet.

import SwiftUI
import UIKit
import UniformTypeIdentifiers

struct ColourSheet: View {
    @Bindable var studio: Studio
    @Environment(\.dismiss) private var dismiss
    @State private var mode = 0
    @State private var palette: [Color] = []
    @State private var showImporter = false
    @State private var showExporter = false
    @State private var exportData = Data()
    @State private var exportType = "gpl"

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            HStack(spacing: 12) {
                BrandMark(size: 26, line: Brand.paper)
                Text("Colours")
                    .font(.system(size: 24, weight: .bold, design: .rounded))
                    .foregroundStyle(Brand.paper)
                Spacer()
                Button("Done") { dismiss() }
                    .buttonStyle(.borderedProminent)
                    .tint(Brand.vermilion)
            }

            Picker("View", selection: $mode) {
                Text("Grid").tag(0)
                Text("Sliders").tag(1)
            }
            .pickerStyle(.segmented)

            Group {
                if mode == 0 {
                    ColourGrid(studio: studio)
                } else {
                    ColourSliders(studio: studio)
                }
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
            .animation(.easeOut(duration: 0.18), value: mode)

            if !palette.isEmpty {
                ScrollView(.horizontal, showsIndicators: false) {
                    HStack(spacing: 6) {
                        ForEach(Array(palette.enumerated()), id: \.offset) { _, swatch in
                            Circle()
                                .fill(swatch)
                                .frame(width: 26, height: 26)
                                .overlay(Circle().stroke(Color.white.opacity(0.18), lineWidth: 1))
                                .onTapGesture { studio.color = swatch }
                        }
                    }
                }
                .frame(height: 30)
            }

            footer
        }
        .padding(20)
        .background(Brand.carbon)
        .preferredColorScheme(.dark)
        .presentationDetents([.large])
        .presentationDragIndicator(.visible)
        .fileImporter(
            isPresented: $showImporter,
            allowedContentTypes: ["gpl", "ase"].compactMap { UTType(filenameExtension: $0) },
            allowsMultipleSelection: false
        ) { result in
            if case .success(let urls) = result, let url = urls.first {
                importPalette(url)
            }
        }
        .fileExporter(
            isPresented: $showExporter,
            document: PaletteFile(data: exportData),
            contentType: UTType(filenameExtension: exportType) ?? .data,
            defaultFilename: "drawkit.\(exportType)"
        ) { _ in }
    }

    private var footer: some View {
        HStack(spacing: 8) {
            Circle()
                .fill(studio.color)
                .frame(width: 32, height: 32)
                .overlay(Circle().stroke(Color.white.opacity(0.2), lineWidth: 1))
            Button {
                studio.rememberColour()
            } label: {
                Image(systemName: "plus")
                    .font(.system(size: 13, weight: .semibold))
                    .foregroundStyle(Brand.paper)
                    .frame(width: 32, height: 32)
                    .background(Brand.raised, in: Circle())
            }
            .buttonStyle(.plain)
            ForEach(Array(studio.recent.prefix(7).enumerated()), id: \.offset) { _, swatch in
                Circle()
                    .fill(swatch)
                    .frame(width: 28, height: 28)
                    .overlay(Circle().stroke(Color.white.opacity(0.18), lineWidth: 1))
                    .onTapGesture {
                        withAnimation(.spring(response: 0.3, dampingFraction: 0.8)) {
                            studio.color = swatch
                        }
                    }
            }
            Spacer()
            Menu {
                Button("Import palette…") { showImporter = true }
                Divider()
                Button("Export GIMP palette…") {
                    exportPalette(format: 0, extension: "gpl")
                }
                Button("Export Adobe palette…") {
                    exportPalette(format: 1, extension: "ase")
                }
            } label: {
                Image(systemName: "swatchpalette")
                    .font(.system(size: 15))
                    .foregroundStyle(Brand.paper.opacity(0.8))
                    .frame(width: 32, height: 32)
                    .background(Brand.raised, in: Circle())
            }
            .menuStyle(.borderlessButton)
            .fixedSize()
            Button {
                studio.sampleArtwork()
            } label: {
                Image(systemName: "sparkles")
                    .font(.system(size: 15))
                    .foregroundStyle(Brand.paper.opacity(0.8))
                    .frame(width: 32, height: 32)
                    .background(Brand.raised, in: Circle())
            }
            .buttonStyle(.plain)
            .help("Sample the artwork")
        }
    }
    private func importPalette(_ url: URL) {
        let access = url.startAccessingSecurityScopedResource()
        defer { if access { url.stopAccessingSecurityScopedResource() } }
        var values = [Float](repeating: 0, count: 256 * 3)
        let count = values.withUnsafeMutableBufferPointer { buffer -> Int32 in
            guard let base = buffer.baseAddress else { return 0 }
            return url.path.withCString {
                dk_read_palette($0, base, UInt32(buffer.count / 3))
            }
        }
        guard count > 0 else {
            UINotificationFeedbackGenerator().notificationOccurred(.error)
            return
        }
        var imported: [Color] = []
        for index in 0..<Int(count) {
            imported.append(
                Color(
                    .sRGB,
                    red: Double(values[index * 3]),
                    green: Double(values[index * 3 + 1]),
                    blue: Double(values[index * 3 + 2]),
                    opacity: 1))
        }
        palette = imported
        studio.recent = Array(imported.prefix(10))
        if let first = imported.first { studio.color = first }
    }

    private func exportPalette(format: Int32, extension pathExtension: String) {
        let colors = palette.isEmpty ? studio.recent : palette
        guard !colors.isEmpty else { return }
        var values: [Float] = []
        for entry in colors {
            var red: CGFloat = 0
            var green: CGFloat = 0
            var blue: CGFloat = 0
            var alpha: CGFloat = 1
            UIColor(entry).getRed(&red, green: &green, blue: &blue, alpha: &alpha)
            values.append(Float(red))
            values.append(Float(green))
            values.append(Float(blue))
        }
        let url = FileManager.default.temporaryDirectory
            .appendingPathComponent("drawkit.\(pathExtension)")
        let saved = values.withUnsafeBufferPointer { buffer -> Int32 in
            guard let base = buffer.baseAddress else { return 0 }
            return url.path.withCString {
                dk_write_palette($0, base, UInt32(buffer.count / 3), format)
            }
        }
        guard saved != 0, let data = try? Data(contentsOf: url) else {
            UINotificationFeedbackGenerator().notificationOccurred(.error)
            return
        }
        exportData = data
        exportType = pathExtension
        showExporter = true
    }

}

private struct ColourGrid: View {
    @Bindable var studio: Studio
    private let columns = 12
    private let rows = 7

    var body: some View {
        GeometryReader { geometry in
            Canvas { context, size in
                let cell = CGSize(
                    width: size.width / CGFloat(columns),
                    height: size.height / CGFloat(rows + 1))
                for column in 0..<columns {
                    let grey = 1 - Double(column) / Double(columns - 1)
                    context.fill(
                        rect(column, 0, cell),
                        with: .color(Color(white: grey)))
                }
                for row in 0..<rows {
                    for column in 0..<columns {
                        context.fill(
                            rect(column, row + 1, cell),
                            with: .color(cellColor(column, row)))
                    }
                }
                if let selected = selectedCell() {
                    let frame = rect(selected.0, selected.1, cell)
                        .boundingRect.insetBy(dx: 2, dy: 2)
                    context.stroke(Path(frame), with: .color(.white), lineWidth: 2.5)
                }
            }
            .contentShape(Rectangle())
            .gesture(
                DragGesture(minimumDistance: 0)
                    .onChanged { value in pick(value.location, in: geometry.size) })
        }
        .aspectRatio(Double(columns) / Double(rows + 1), contentMode: .fit)
        .clipShape(RoundedRectangle(cornerRadius: 12))
    }

    private func rect(_ column: Int, _ row: Int, _ cell: CGSize) -> Path {
        Path(
            CGRect(
                x: CGFloat(column) * cell.width, y: CGFloat(row) * cell.height,
                width: cell.width, height: cell.height))
    }

    private func cellColor(_ column: Int, _ row: Int) -> Color {
        let hue = (Double(column) + 0.5) / Double(columns)
        let lightness = 0.16 + Double(row) / Double(rows - 1) * 0.74
        return hsl(hue, 1, lightness)
    }

    private func pick(_ point: CGPoint, in size: CGSize) {
        guard size.width > 0, size.height > 0 else { return }
        let cellWidth = size.width / CGFloat(columns)
        let cellHeight = size.height / CGFloat(rows + 1)
        let column = min(max(Int(point.x / cellWidth), 0), columns - 1)
        let row = min(max(Int(point.y / cellHeight), 0), rows)
        studio.color =
            row == 0
            ? Color(white: 1 - Double(column) / Double(columns - 1))
            : cellColor(column, row - 1)
        studio.rememberColour()
    }

    private func selectedCell() -> (Int, Int)? {
        let hsb = studio.color.hsb
        if hsb.saturation < 0.10 {
            return (min(max(Int((1 - hsb.brightness) * Double(columns - 1)), 0), columns - 1), 0)
        }
        let column = min(Int(hsb.hue * Double(columns)), columns - 1)
        let lightness = hsb.brightness * (1 - hsb.saturation / 2)
        let row = Int((((lightness - 0.16) / 0.74) * Double(rows - 1)).rounded())
        return (column, min(max(row, 0), rows - 1) + 1)
    }
}

private struct ColourSliders: View {
    @Bindable var studio: Studio

    var body: some View {
        let hsb = studio.color.hsb
        VStack(spacing: 10) {
            row("Hue", value: hsb.hue * 360, range: 0...360, display: "\(Int(hsb.hue * 360))°") {
                studio.color = hsv(
                    hue: $0 / 360, saturation: hsb.saturation,
                    brightness: hsb.brightness, alpha: studio.color.uiAlpha)
            }
            row(
                "Saturation", value: hsb.saturation, range: 0...1,
                display: "\(Int(hsb.saturation * 100))%"
            ) {
                studio.color = hsv(
                    hue: hsb.hue, saturation: $0,
                    brightness: hsb.brightness, alpha: studio.color.uiAlpha)
            }
            row(
                "Brightness", value: hsb.brightness, range: 0...1,
                display: "\(Int(hsb.brightness * 100))%"
            ) {
                studio.color = hsv(
                    hue: hsb.hue, saturation: hsb.saturation,
                    brightness: $0, alpha: studio.color.uiAlpha)
            }
            row(
                "Opacity", value: studio.color.uiAlpha, range: 0.05...1,
                display: "\(Int(studio.color.uiAlpha * 100))%"
            ) {
                studio.color = studio.color.opacity($0)
            }
            Spacer(minLength: 0)
        }
    }

    private func row(
        _ label: String, value: Double, range: ClosedRange<Double>,
        display: String, onChange: @escaping @MainActor (Double) -> Void
    ) -> some View {
        HStack(spacing: 12) {
            Text(label)
                .font(.system(size: 12))
                .foregroundStyle(Brand.muted)
                .frame(width: 86, alignment: .leading)
            Slider(
                value: Binding(get: { value }, set: { @MainActor newValue in onChange(newValue) }),
                in: range
            )
            .tint(Brand.vermilion)
            Text(display)
                .font(.system(size: 11).monospacedDigit())
                .foregroundStyle(Brand.paper.opacity(0.65))
                .frame(width: 44, alignment: .trailing)
        }
    }
}

extension Color {
    fileprivate var uiAlpha: Double {
        Double(UIColor(self).cgColor.alpha)
    }

    fileprivate var hsb: (hue: Double, saturation: Double, brightness: Double) {
        var hue: CGFloat = 0
        var saturation: CGFloat = 0
        var brightness: CGFloat = 0
        var alpha: CGFloat = 1
        UIColor(self).getHue(&hue, saturation: &saturation, brightness: &brightness, alpha: &alpha)
        return (Double(hue), Double(saturation), Double(brightness))
    }
}

private func hsl(_ hue: Double, _ saturation: Double, _ lightness: Double) -> Color {
    let c = (1 - abs(2 * lightness - 1)) * saturation
    let h = (hue - floor(hue)) * 6
    let x = c * (1 - abs(h.truncatingRemainder(dividingBy: 2) - 1))
    let rgb: (Double, Double, Double)
    switch Int(h) {
    case 0: rgb = (c, x, 0)
    case 1: rgb = (x, c, 0)
    case 2: rgb = (0, c, x)
    case 3: rgb = (0, x, c)
    case 4: rgb = (x, 0, c)
    default: rgb = (c, 0, x)
    }
    let m = lightness - c / 2
    return Color(red: rgb.0 + m, green: rgb.1 + m, blue: rgb.2 + m)
}

private func hsv(hue: Double, saturation: Double, brightness: Double, alpha: Double) -> Color {
    let ui = UIColor(hue: hue, saturation: saturation, brightness: brightness, alpha: alpha)
    return Color(ui)
}

struct PaletteFile: FileDocument {
    static var readableContentTypes: [UTType] {
        [UTType(filenameExtension: "gpl") ?? .data, UTType(filenameExtension: "ase") ?? .data]
    }

    let data: Data

    init(data: Data) { self.data = data }

    init(configuration: ReadConfiguration) throws {
        data = configuration.file.regularFileContents ?? Data()
    }

    func fileWrapper(configuration: WriteConfiguration) throws -> FileWrapper {
        FileWrapper(regularFileWithContents: data)
    }
}
