// The iPad layout, after Procreate: paper edge to edge, a floating top bar, a property card at the
// left edge, a colour well at the bottom right and the layers in a sheet. Chrome is temporary; the
// painting is not.

import SwiftUI
import UniformTypeIdentifiers

struct ContentView: View {
    @Bindable var studio: Studio
    @Environment(AppSettings.self) private var settings
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var dropTargeted = false

    var body: some View {
        ZStack {
            CanvasView(studio: studio, revision: studio.revision) {
                studio.tool = studio.tool == .eraser ? .brush : .eraser
            }
            .ignoresSafeArea()

            topBar
                .padding(.horizontal, 12)
                .padding(.top, 8)
                .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)

            if !studio.isBlank {
                ToolCard(studio: studio)
                    .frame(width: 250)
                    .padding(.leading, 16)
                    .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .leading)
            }

            if studio.isBlank {
                StartHint()
            }

            statusLine
                .padding(.leading, 20)
                .padding(.bottom, 16)
                .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .bottomLeading)

            colourWell
                .padding(.trailing, 20)
                .padding(.bottom, 16)
                .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .bottomTrailing)

            if !settings.hasCompletedOnboarding {
                OnboardingView {
                    settings.hasCompletedOnboarding = true
                }
            }
        }
        .background(Brand.ink)
        .overlay {
            if dropTargeted {
                RoundedRectangle(cornerRadius: 12, style: .continuous)
                    .stroke(Brand.vermilion, lineWidth: 2)
                    .padding(10)
                    .allowsHitTesting(false)
            }
        }
        .onDrop(of: [UTType.fileURL], isTargeted: $dropTargeted) { providers in
            guard let provider = providers.first else { return false }
            _ = provider.loadDataRepresentation(forTypeIdentifier: UTType.fileURL.identifier) {
                data, _ in
                guard let data, let url = URL(dataRepresentation: data, relativeTo: nil)
                else { return }
                DispatchQueue.main.async { _ = studio.importDocument(url) }
            }
            return true
        }
        .preferredColorScheme(.dark)
        .sheet(isPresented: $studio.showColours) {
            ColourSheet(studio: studio)
        }
        .sheet(isPresented: $studio.showLayers) {
            LayersSheet(studio: studio)
        }
        .sheet(isPresented: $studio.showPencilKit) {
            PencilKitSheet(studio: studio)
        }
        .sheet(isPresented: $studio.showSettings) {
            SettingsSheet(settings: settings, studio: studio)
        }
        .fileImporter(
            isPresented: $studio.showImport,
            allowedContentTypes: ["kra", "psd"].compactMap { UTType(filenameExtension: $0) },
            allowsMultipleSelection: false
        ) { result in
            if case .success(let urls) = result, let url = urls.first {
                studio.importDocument(url)
            }
        }
        .fileExporter(
            isPresented: $studio.showExporter,
            document: studio.exportPayload.map(ExportFile.init),
            contentType: UTType(filenameExtension: studio.exportType) ?? .data,
            defaultFilename: studio.exportName
        ) { _ in
            studio.exportPayload = nil
        }
    }

    private var topBar: some View {
        HStack(spacing: 6) {
            BrandMark(size: 26)
            Text("DrawKit")
                .font(.system(size: 17, weight: .semibold, design: .rounded))
                .foregroundStyle(Brand.paper)
                .padding(.trailing, 8)

            ForEach(IOSTool.allCases) { tool in
                toolButton(tool)
            }

            Spacer(minLength: 8)

            iconButton("arrow.uturn.backward", help: "Undo", enabled: studio.canUndo) {
                studio.undo()
            }
            .keyboardShortcut("z", modifiers: .command)
            iconButton("arrow.uturn.forward", help: "Redo", enabled: studio.canRedo) {
                studio.redo()
            }
            .keyboardShortcut("z", modifiers: [.command, .shift])
            iconButton("square.3.layers.3d", help: "Layers") {
                studio.showLayers = true
            }
            .keyboardShortcut("l", modifiers: .command)
            Menu {
                Button("Import Krita or PSD…") { studio.showImport = true }
                Button("PencilKit sketch…") { studio.showPencilKit = true }
                Divider()
                Button("Export Krita…") { studio.prepareExport("kra") }
                Button("Export Photoshop…") { studio.prepareExport("psd") }
                Button("Export EXR…") { studio.prepareExport("exr") }
                Button("Save project (.dkp)") { studio.prepareExport("dkp") }
                Button("Save stroke tape…") { studio.prepareExport("dkp") }
                Divider()
                Button("Clear layer") { studio.clearLayer() }
                Button("Add layer") { studio.addLayer() }
                Button("Remove layer") { studio.removeLayer() }
                Divider()
                Button(settings.t(.settingsTitle)) { studio.showSettings = true }
            } label: {
                Image(systemName: "ellipsis.circle")
                    .font(.system(size: 18, weight: .medium))
                    .foregroundStyle(Brand.paper.opacity(0.8))
                    .frame(width: 44, height: 44)
            }
        }
        .padding(.horizontal, 10)
        .frame(height: 56)
        .background(Brand.carbon.opacity(0.94), in: RoundedRectangle(cornerRadius: 20))
        .overlay(RoundedRectangle(cornerRadius: 20)
            .stroke(Color.white.opacity(0.07), lineWidth: 1))
        .shadow(color: .black.opacity(0.35), radius: 14, y: 6)
    }

    private func toolButton(_ tool: IOSTool) -> some View {
        Button {
            withAnimation(reduceMotion ? nil : .spring(response: 0.26, dampingFraction: 0.74)) {
                studio.tool = tool
            }
        } label: {
            Image(systemName: tool.symbol)
                .font(.system(size: 17, weight: .medium))
                .foregroundStyle(studio.tool == tool ? Brand.paper : Brand.paper.opacity(0.65))
                .frame(width: 44, height: 44)
                .background(
                    RoundedRectangle(cornerRadius: 12)
                        .fill(studio.tool == tool ? Brand.vermilion : Color.clear)
                )
        }
        .buttonStyle(.plain)
        .keyboardShortcut(tool.shortcut, modifiers: [])
        .help(tool.rawValue)
    }

    private func iconButton(_ symbol: String, help: String, enabled: Bool = true,
                            action: @escaping () -> Void) -> some View {
        Button(action: action) {
            Image(systemName: symbol)
                .font(.system(size: 18, weight: .medium))
                .foregroundStyle(Brand.paper.opacity(enabled ? 0.8 : 0.25))
                .frame(width: 44, height: 44)
        }
        .buttonStyle(.plain)
        .disabled(!enabled)
        .help(help)
    }

    private var statusLine: some View {
        VStack(alignment: .leading, spacing: 2) {
            Text("DrawKit")
                .font(.system(size: 13, weight: .semibold))
                .foregroundStyle(Brand.paper)
                .tracking(0.5)
            Text("\(studio.width) × \(studio.height) · \(studio.layers.count) " +
                 (studio.layers.count == 1 ? "layer" : "layers"))
                .font(.system(size: 10))
                .foregroundStyle(Brand.muted)
        }
    }

    private var colourWell: some View {
        Button {
            studio.showColours = true
        } label: {
            Circle()
                .fill(studio.color)
                .frame(width: 56, height: 56)
                .overlay(Circle().stroke(Color.white.opacity(0.35), lineWidth: 2))
                .shadow(color: .black.opacity(0.4), radius: 10, y: 4)
        }
        .buttonStyle(.plain)
        .help("Colours")
    }
}

private struct ToolCard: View {
    @Bindable var studio: Studio

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack {
                Text(studio.tool.rawValue)
                    .font(.system(size: 15, weight: .semibold))
                    .foregroundStyle(Brand.paper)
                Spacer()
                Text(studio.tool == .fill || studio.tool == .eyedropper ? "tap" : "\(Int(studio.brushSize)) px")
                    .font(.system(size: 11))
                    .foregroundStyle(Brand.muted)
            }
            if studio.tool == .eyedropper {
                Text("Tap the canvas to sample a colour")
                    .font(.system(size: 11))
                    .foregroundStyle(Brand.muted)
            } else if studio.tool == .fill {
                row("Tolerance", value: $studio.tolerance, range: 0.01...0.6,
                    display: "\(Int(studio.tolerance * 100))%")
                row("Gap", value: $studio.fillClose, range: 0...4,
                    display: "\(Int(studio.fillClose))")
                row("Grow", value: $studio.fillExpand, range: 0...8,
                    display: "\(Int(studio.fillExpand))")
                row("Soft", value: $studio.fillFeather, range: 0...12,
                    display: "\(Int(studio.fillFeather))")
            } else {
                row("Size", value: $studio.brushSize, range: 1...90,
                    display: "\(Int(studio.brushSize)) px")
                row("Opacity", value: $studio.opacity, range: 0.05...1,
                    display: "\(Int(studio.opacity * 100))%")
                HStack {
                    Text("Mirror")
                        .font(.system(size: 11))
                        .foregroundStyle(Brand.paper.opacity(0.7))
                    Picker("", selection: $studio.mirror) {
                        Text("Off").tag(0)
                        Text("H").tag(1)
                        Text("V").tag(2)
                        Text("Both").tag(3)
                    }
                    .pickerStyle(.segmented)
                    .labelsHidden()
                }
            }
            row("Steady", value: $studio.stabilization, range: 0...0.9,
                display: "\(Int(studio.stabilization * 100))%")
        }
        .padding(14)
        .background(Brand.carbon.opacity(0.92), in: RoundedRectangle(cornerRadius: 18))
        .overlay(RoundedRectangle(cornerRadius: 18)
            .stroke(Color.white.opacity(0.06), lineWidth: 1))
        .shadow(color: .black.opacity(0.3), radius: 10, y: 4)
    }

    private func row(_ label: String, value: Binding<Double>,
                     range: ClosedRange<Double>, display: String) -> some View {
        VStack(spacing: 0) {
            HStack {
                Text(label)
                    .font(.system(size: 11))
                    .foregroundStyle(Brand.paper.opacity(0.7))
                Spacer()
                Text(display)
                    .font(.system(size: 10))
                    .foregroundStyle(Brand.muted)
            }
            Slider(value: value, in: range)
                .tint(Brand.vermilion)
        }
    }
}

private struct ExportFile: FileDocument {
    static var readableContentTypes: [UTType] { [.data] }

    var data: Data

    init(_ data: Data) {
        self.data = data
    }

    init(configuration: ReadConfiguration) throws {
        data = configuration.file.regularFileContents ?? Data()
    }

    func fileWrapper(configuration: WriteConfiguration) throws -> FileWrapper {
        FileWrapper(regularFileWithContents: data)
    }
}
