// The animation strip. A first version keeps each frame as a whole-document snapshot; the core
// owns the pixels and undo stack, the shell owns the thumbnails and the onion controls.

import SwiftUI

struct FrameSection: View {
    @Bindable var model: PaintModel

    var body: some View {
        PanelCard("Animation") {
            ScrollView(.horizontal, showsIndicators: false) {
                HStack(spacing: 6) {
                    ForEach(0..<model.frameCount, id: \.self) { index in
                        Button {
                            model.selectFrame(index)
                        } label: {
                            ZStack {
                                RoundedRectangle(cornerRadius: 6, style: .continuous)
                                    .fill(Brand.ink)
                                if index < model.frameImages.count,
                                   let image = model.frameImages[index] {
                                    Image(decorative: image, scale: 1)
                                        .resizable()
                                        .aspectRatio(contentMode: .fit)
                                }
                            }
                            .frame(width: 44, height: 44)
                            .overlay(
                                RoundedRectangle(cornerRadius: 6, style: .continuous)
                                    .stroke(index == model.activeFrame
                                            ? Brand.vermilion : Color.white.opacity(0.10),
                                            lineWidth: index == model.activeFrame ? 2 : 1)
                            )
                        }
                        .buttonStyle(.plain)
                        .help("Frame \(index + 1)")
                    }
                }
                .padding(.vertical, 2)
            }

            HStack(spacing: 8) {
                Button {
                    model.togglePlayback()
                } label: {
                    Image(systemName: model.isPlaying ? "pause.fill" : "play.fill")
                }
                Button("<") { model.stepFrame(-1) }
                Button(">") { model.stepFrame(1) }
                Stepper("FPS \(model.fps)", value: $model.fps, in: 1...60)
            }
            .controlSize(.small)
            HStack(spacing: 8) {
                Button("+") { model.addFrame(duplicate: false) }
                Button("Duplicate") { model.addFrame(duplicate: true) }
                Button("−") { model.removeFrame() }
            }
            .controlSize(.small)

            Toggle("Onion skin", isOn: $model.onionEnabled)
                .font(.caption)
                .toggleStyle(.checkbox)
            HStack {
                Stepper("Before \(model.onionBefore)", value: $model.onionBefore, in: 0...3)
                Stepper("After \(model.onionAfter)", value: $model.onionAfter, in: 0...3)
            }
            .font(.caption)
        }
        .onAppear { model.refreshFrames() }
    }
}
