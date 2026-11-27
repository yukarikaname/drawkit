// The replay scrubber. It is shown only while a recording exists.

import SwiftUI

struct ReplayBar: View {
    let model: PaintModel

    var body: some View {
        HStack(spacing: 10) {
            Text("Replay").font(.caption).foregroundStyle(.secondary)
            PenSlider(
                value: Binding(
                    get: { Double(model.previewMs ?? model.recordingMs) },
                    set: {
                        model.previewMs = UInt64($0)
                        model.refresh()
                    }
                ),
                range: 0...Double(max(model.recordingMs, 1)),
                width: 360
            )
            Button("Live") {
                model.previewMs = nil
                model.refresh()
            }
            Button {
                model.exportTimelapse()
            } label: {
                Image(systemName: "film")
            }
            .help("Export this tape as an MP4 timelapse")
            Text(
                "\(model.recordingMs / 1000).\(String(format: "%03d", model.recordingMs % 1000)) s"
            )
            .font(.caption.monospacedDigit())
            .foregroundStyle(.secondary)
        }
        .padding(.horizontal, 14)
        .frame(height: 34)
        .background(
            RoundedRectangle(cornerRadius: 10, style: .continuous)
                .fill(Brand.carbon)
                .overlay(
                    RoundedRectangle(cornerRadius: 10)
                        .stroke(.white.opacity(0.06), lineWidth: 1))
        )
    }
}
