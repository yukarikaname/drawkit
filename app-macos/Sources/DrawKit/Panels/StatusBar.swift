// The status strip. It says what the canvas is, what the tool will do, and which shortcuts exist,
// in the smallest type on screen.

import SwiftUI

struct StatusBar: View {
    let width: Int
    let height: Int
    let tool: Tool
    let size: Double
    let stabilization: Double
    let colorSpace: PaintColorSpace
    let hdr: Bool
    let brush: BrushKind
    let dirty: Bool
    let status: StatusMessage?
    let shortcuts: ShortcutStore
    @Binding var zoom: Double
    let rotation: Double
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    var body: some View {
        HStack(spacing: 16) {
            Text("\(width) × \(height)")
            Text(tool.rawValue)
            if tool == .brush || tool == .eraser {
                Text(brush.title)
                Text("\(Int(size)) px")
                Text(String(format: "smooth %.1f", stabilization))
            }
            Text(hdr ? "\(colorSpace.title) · HDR" : colorSpace.title)
            if dirty {
                Text("•")
                    .foregroundStyle(Brand.vermilion)
                    .help("Unsaved changes")
            }
            if let status {
                Label(status.text,
                      systemImage: status.isError ? "exclamationmark.triangle.fill"
                                                  : "checkmark.circle.fill")
                    .foregroundStyle(status.isError ? Color.red : Color.green)
                    .transition(.opacity.combined(with: .move(edge: .bottom)))
            }
            Spacer()
            Button { zoom = max(0.1, zoom / 1.25) } label: {
                Image(systemName: "minus.magnifyingglass")
                    .frame(width: 24, height: 24)
            }
            .buttonStyle(.borderless)
            .foregroundStyle(Brand.muted)
                .help("Zoom out")
            Text("\(Int(zoom * 100))%")
                .frame(width: 38, alignment: .center)
            Button { zoom = min(32, zoom * 1.25) } label: {
                Image(systemName: "plus.magnifyingglass")
                    .frame(width: 24, height: 24)
            }
            .buttonStyle(.borderless)
            .foregroundStyle(Brand.muted)
                .help("Zoom in")
            if abs(rotation) >= 0.5 {
                Text(String(format: "%.0f°", rotation))
                    .help("Canvas rotation")
            }
        }
        .font(.caption2.monospacedDigit())
        .foregroundStyle(Brand.muted)
        .padding(.horizontal, 4)
        .frame(height: 24)
        .animation(Motion.panel.resolving(reduceMotion: reduceMotion), value: status)
    }
}
