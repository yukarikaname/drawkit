// The first screen: choose a canvas size, reopen a recent painting, or open a file. DrawKit does not
// assume a 1024-square canvas; that was only the size the shell used while the core was growing.

import SwiftUI
import AppKit
import UniformTypeIdentifiers

struct WelcomeView: View {
    @Environment(AppSettings.self) private var settings
    let model: PaintModel
    let onCreate: (Int, Int, PaintColorSpace, Bool) -> Void
    let onOpened: () -> Void

    @State private var width = 1024
    @State private var height = 1024
    @State private var colorSpace: PaintColorSpace = .sRGB
    @State private var hdr = false
    @State private var recent: [URL] = []
    @State private var thumbnails: [URL: NSImage] = [:]
    @State private var appeared = false
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    private let presets: [(LocalizedStringKey, String, Int, Int)] = [
        ("Sketch", "1024 × 1024", 1024, 1024),
        ("Large", "2048 × 2048", 2048, 2048),
        ("4K", "4096 × 4096", 4096, 4096),
        ("Portrait", "1080 × 1920", 1080, 1920),
        ("Landscape", "1920 × 1080", 1920, 1080),
        ("A4", "2480 × 3508", 2480, 3508),
    ]

    private var validSize: Bool {
        (16...8192).contains(width) && (16...8192).contains(height)
    }

    var body: some View {
        ZStack {
            Brand.ink
            HStack(alignment: .top, spacing: 28) {
                newPainting
                    .frame(width: 470)
                    .opacity(appeared ? 1 : 0)
                    .offset(y: appeared ? 0 : 10)
                recentPainting
                    .frame(width: 400)
                    .opacity(appeared ? 1 : 0)
                    .offset(y: appeared ? 0 : 16)
            }
            .padding(38)
        }
        .onAppear {
            withAnimation(Motion.panel.resolving(reduceMotion: reduceMotion)) {
                appeared = true
            }
            recent = RecentDocuments.all()
            thumbnails = Dictionary(uniqueKeysWithValues: recent.compactMap { url in
                RecentDocuments.thumbnail(for: url).map { (url, $0) }
            })
        }
    }

    private var newPainting: some View {
        VStack(alignment: .leading, spacing: 22) {
            HStack(spacing: 14) {
                BrandMark(size: 58, line: Brand.paper)
                VStack(alignment: .leading, spacing: 2) {
                    Text("DrawKit")
                        .font(.system(size: 34, weight: .bold, design: .rounded))
                        .foregroundStyle(Brand.paper)
                    Text("A practical painting kit.")
                        .font(.system(size: 13))
                        .foregroundStyle(Brand.muted)
                }
            }

            Text("New painting")
                .font(.system(size: 17, weight: .semibold))
                .foregroundStyle(Brand.paper)

            LazyVGrid(columns: [GridItem(.flexible()), GridItem(.flexible())], spacing: 8) {
                ForEach(presets.indices, id: \.self) { index in
                    presetButton(presets[index])
                }
            }

            VStack(alignment: .leading, spacing: 8) {
                Text("Colour space")
                    .font(.system(size: 12))
                    .foregroundStyle(Brand.muted)
                HStack(spacing: 6) {
                    ForEach(PaintColorSpace.allCases) { space in
                        colorSpaceChoice(space)
                    }
                }
                HStack(spacing: 8) {
                    Text("HDR · extended range")
                        .font(.system(size: 12))
                        .foregroundStyle(Brand.paper.opacity(0.8))
                    PenToggle(isOn: $hdr, help: "Keeps highlights above paper white; export EXR to keep them.")
                }
                Text("Keeps highlights above paper white; export EXR to keep them.")
                    .font(.system(size: 10))
                    .foregroundStyle(Brand.muted)
            }

            VStack(alignment: .leading, spacing: 8) {
                Text("Custom size")
                    .font(.system(size: 12))
                    .foregroundStyle(Brand.muted)
                HStack(spacing: 8) {
                    sizeField("Width", value: $width)
                    Text("×")
                        .foregroundStyle(Brand.muted)
                    sizeField("Height", value: $height)
                    Text("px")
                        .font(.system(size: 12))
                        .foregroundStyle(Brand.muted)
                }
            }

            Button {
                onCreate(width, height, colorSpace, hdr)
            } label: {
                Text("Start painting")
                    .frame(maxWidth: .infinity)
            }
            .buttonStyle(.borderedProminent)
            .tint(Brand.vermilion)
            .controlSize(.large)
            .keyboardShortcut(.defaultAction)
            .help("Create this canvas (Return)")
            .disabled(!validSize)

            Text("16–8192 pixels per side. Undo, the brush tape and imports scale with this canvas.")
                .font(.caption2)
                .foregroundStyle(Brand.muted)
        }
        .padding(26)
        .background(Brand.carbon.opacity(0.8), in: RoundedRectangle(cornerRadius: 24))
        .overlay(RoundedRectangle(cornerRadius: 24)
            .stroke(Color.white.opacity(0.07), lineWidth: 1))
    }

    private var recentPainting: some View {
        VStack(alignment: .leading, spacing: 14) {
            HStack {
                Text("Recent")
                    .font(.system(size: 17, weight: .semibold))
                    .foregroundStyle(Brand.paper)
                Spacer()
                Button("Open…") { open() }
                    .controlSize(.small)
            }

            if recent.isEmpty {
                VStack(spacing: 8) {
                    Image(systemName: "photo.on.rectangle.angled")
                        .font(.system(size: 28, weight: .light))
                    Text("No recent paintings")
                        .font(.system(size: 12))
                }
                .foregroundStyle(Brand.muted)
                .frame(maxWidth: .infinity, minHeight: 150)
            } else {
                ScrollView {
                    LazyVStack(spacing: 6) {
                        ForEach(recent, id: \.self) { url in
                            recentRow(url)
                        }
                    }
                }
                .frame(maxHeight: 340)
            }

            Spacer(minLength: 0)

            Button {
                open()
            } label: {
                Label("Open project or image", systemImage: "folder")
                    .frame(maxWidth: .infinity)
            }
            .buttonStyle(.bordered)
            .controlSize(.large)

            Text("Projects (.rpp), Krita, Photoshop, PNG and JPEG open at their own size.")
                .font(.caption2)
                .foregroundStyle(Brand.muted)
        }
        .padding(24)
        .background(Brand.carbon.opacity(0.55), in: RoundedRectangle(cornerRadius: 24))
        .overlay(RoundedRectangle(cornerRadius: 24)
            .stroke(Color.white.opacity(0.06), lineWidth: 1))
    }

    private func colorSpaceChoice(_ space: PaintColorSpace) -> some View {
        let selected = colorSpace == space
        return Button {
            withAnimation(.spring(response: 0.26, dampingFraction: 0.78)) {
                colorSpace = space
            }
        } label: {
            VStack(alignment: .leading, spacing: 2) {
                Text(space.title)
                    .font(.system(size: 12, weight: .semibold))
                    .foregroundStyle(Brand.paper)
                Text(colorSpaceSubtitle(space))
                    .font(.system(size: 9))
                    .foregroundStyle(Brand.muted)
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(.horizontal, 10)
            .padding(.vertical, 7)
            .background(
                RoundedRectangle(cornerRadius: 9, style: .continuous)
                    .fill(selected ? Brand.vermilion.opacity(0.20)
                          : Brand.raised.opacity(0.48))
            )
            .overlay(
                RoundedRectangle(cornerRadius: 9, style: .continuous)
                    .stroke(selected ? Brand.vermilion.opacity(0.85)
                            : Color.white.opacity(0.04), lineWidth: 1)
            )
        }
        .buttonStyle(.plain)
    }

    private func colorSpaceSubtitle(_ space: PaintColorSpace) -> String {
        switch space {
        case .sRGB: "Web"
        case .displayP3: "Wide screen"
        case .adobeRgb: "Print"
        }
    }

    private func presetButton(_ preset: (LocalizedStringKey, String, Int, Int)) -> some View {
        let active = preset.2 == width && preset.3 == height
        return Button {
            width = preset.2
            height = preset.3
        } label: {
            VStack(alignment: .leading, spacing: 2) {
                Text(preset.0)
                    .font(.system(size: 13, weight: .semibold))
                    .foregroundStyle(Brand.paper)
                Text(preset.1)
                    .font(.system(size: 10).monospacedDigit())
                    .foregroundStyle(Brand.muted)
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(.horizontal, 12)
            .padding(.vertical, 9)
            .background(
                RoundedRectangle(cornerRadius: 10)
                    .fill(active ? Brand.vermilion.opacity(0.22) : Brand.raised.opacity(0.55))
            )
            .overlay(RoundedRectangle(cornerRadius: 10)
                .stroke(active ? Brand.vermilion.opacity(0.8) : Color.clear, lineWidth: 1))
        }
        .buttonStyle(.plain)
    }

    private func sizeField(_ label: String, value: Binding<Int>) -> some View {
        TextField(label, value: value, format: .number)
            .textFieldStyle(.roundedBorder)
            .frame(width: 96)
            .controlSize(.large)
            .multilineTextAlignment(.trailing)
            .onChange(of: value.wrappedValue) { _, newValue in
                if newValue > 8192 { value.wrappedValue = 8192 }
                if newValue < 16 { value.wrappedValue = 16 }
            }
    }

    private func recentRow(_ url: URL) -> some View {
        Button {
            if model.openDocument(url) {
                onOpened()
            }
        } label: {
            HStack(spacing: 10) {
                if let thumbnail = thumbnails[url] {
                    Image(nsImage: thumbnail)
                        .resizable()
                        .interpolation(.high)
                        .frame(width: 36, height: 36)
                        .clipShape(RoundedRectangle(cornerRadius: 7))
                } else {
                    Image(systemName: icon(for: url))
                        .font(.system(size: 16))
                        .foregroundStyle(Brand.vermilion)
                        .frame(width: 36, height: 36)
                        .background(Brand.raised, in: RoundedRectangle(cornerRadius: 7))
                }
                VStack(alignment: .leading, spacing: 1) {
                    Text(url.lastPathComponent)
                        .font(.system(size: 13, weight: .medium))
                        .foregroundStyle(Brand.paper)
                        .lineLimit(1)
                    Text(url.deletingLastPathComponent().path)
                        .font(.system(size: 9))
                        .foregroundStyle(Brand.muted)
                        .lineLimit(1)
                        .truncationMode(.middle)
                }
                Spacer()
                Text(url.pathExtension.uppercased())
                    .font(.system(size: 9, weight: .semibold))
                    .foregroundStyle(Brand.muted)
            }
            .padding(.horizontal, 8)
            .padding(.vertical, 6)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
    }

    private func icon(for url: URL) -> String {
        switch url.pathExtension.lowercased() {
        case "dkp": "paintbrush.pointed"
        case "kra": "k.square"
        case "psd": "p.square"
        default: "photo"
        }
    }

    private func open() {
        let panel = NSOpenPanel()
        panel.allowedContentTypes = ["dkp", "kra", "psd", "png", "jpg", "jpeg"]
            .compactMap { UTType(filenameExtension: $0) }
        panel.allowsMultipleSelection = false
        guard panel.runModal() == .OK, let url = panel.url else { return }
        if model.openDocument(url) {
            onOpened()
        }
    }
}
