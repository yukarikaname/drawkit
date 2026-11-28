// The status strip. It says what the canvas is, what the tool will do, and which shortcuts exist,
// in the smallest type on screen.

import SwiftUI

struct StatusBar: View {
    let width: Int
    let height: Int
    let colorSpace: PaintColorSpace
    let hdr: Bool
    let frameCount: Int
    let activeFrame: Int
    let fps: Int
    let dirty: Bool
    let status: StatusMessage?
    @Binding var zoom: Double
    let rotation: Double
    let onResetRotation: () -> Void
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    var body: some View {
        HStack(spacing: 16) {
            Text("\(width) × \(height)")
                .help("Canvas size")
            Text(hdr ? "\(colorSpace.title) · HDR" : colorSpace.title)
                .help("Working colour space")
            if frameCount > 1 {
                Text("frame \(activeFrame + 1)/\(frameCount) · \(fps) fps")
                    .help("Animation timeline")
            }
            if dirty {
                Text("•")
                    .foregroundStyle(Brand.ash)
                    .help("Unsaved changes")
            }
            if let status {
                Label(
                    status.text,
                    systemImage: status.isError
                        ? "exclamationmark.triangle.fill"
                        : "checkmark.circle.fill"
                )
                .foregroundStyle(status.isError ? Color.red : Color.green)
                .transition(.opacity.combined(with: .move(edge: .bottom)))
            }
            Spacer()
            Button {
                zoom = max(0.1, zoom / 1.25)
            } label: {
                Image(systemName: "minus.magnifyingglass")
                    .frame(width: 24, height: 24)
            }
            .buttonStyle(.borderless)
            .foregroundStyle(Brand.muted)
            .help("Zoom out")
            Text("\(Int(zoom * 100))%")
                .frame(width: 38, alignment: .center)
            Button {
                zoom = min(32, zoom * 1.25)
            } label: {
                Image(systemName: "plus.magnifyingglass")
                    .frame(width: 24, height: 24)
            }
            .buttonStyle(.borderless)
            .foregroundStyle(Brand.muted)
            .help("Zoom in")
            if abs(rotation) >= 0.5 {
                Button {
                    onResetRotation()
                } label: {
                    HStack(spacing: 3) {
                        Text(String(format: "%.0f°", rotation))
                        Image(systemName: "arrow.counterclockwise")
                    }
                }
                .buttonStyle(.borderless)
                .foregroundStyle(Brand.muted)
                .help("Reset canvas rotation")
            }
        }
        .font(.caption2.monospacedDigit())
        .foregroundStyle(Brand.muted)
        .padding(.horizontal, 4)
        .frame(height: 24)
        .animation(Motion.panel.resolving(reduceMotion: reduceMotion), value: status)
    }
}
