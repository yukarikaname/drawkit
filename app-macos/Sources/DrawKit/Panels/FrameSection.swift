// The animation strip. A first version keeps each frame as a whole-document snapshot; the core
// owns the pixels and undo stack, the shell owns the thumbnails and the onion controls.

import SwiftUI

struct FrameSection: View {
    @Bindable var model: PaintModel
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    var body: some View {
        PanelCard("Animation", startsExpanded: model.frameCount > 1) {
            let loop = model.loopRange()
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
                            .overlay(alignment: .bottomTrailing) {
                                Text("\(index + 1)")
                                    .font(.system(size: 8, weight: .semibold,
                                                  design: .monospaced))
                                    .foregroundStyle(.white.opacity(0.9))
                                    .padding(.horizontal, 3)
                                    .padding(.vertical, 1)
                                    .background(.black.opacity(0.55),
                                                in: RoundedRectangle(cornerRadius: 3))
                                    .padding(2)
                            }
                            .overlay(
                                RoundedRectangle(cornerRadius: 6, style: .continuous)
                                    .stroke(index == model.activeFrame
                                            ? Brand.vermilion : Color.white.opacity(0.10),
                                            lineWidth: index == model.activeFrame ? 2 : 1)
                            )
                            .opacity(index >= loop.0 && index < loop.1 ? 1 : 0.42)
                        }
                        .buttonStyle(.plain)
                        .transition(.scale(scale: 0.86).combined(with: .opacity))
                        .help(index >= loop.0 && index < loop.1
                              ? "Frame \(index + 1)"
                              : "Frame \(index + 1) (outside the loop)")
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
                Button("<") {
                    withAnimation(Motion.select.resolving(reduceMotion: reduceMotion)) {
                        model.stepFrame(-1)
                    }
                }
                Button(">") {
                    withAnimation(Motion.select.resolving(reduceMotion: reduceMotion)) {
                        model.stepFrame(1)
                    }
                }
                Stepper("FPS \(model.fps)", value: $model.fps, in: 1...60)
            }
            .controlSize(.small)
            HStack(spacing: 8) {
                Stepper("Hold \(model.frameDuration) ms", value: $model.frameDuration,
                        in: 10...2_000, step: 10)
                Toggle("Loop", isOn: $model.loopEnabled)
                    .toggleStyle(.checkbox)
            }
            .controlSize(.small)
            HStack(spacing: 8) {
                Stepper("In \(model.loopStart + 1)", value: $model.loopStart,
                        in: 0...max(0, min(model.loopEnd - 1, model.frameCount - 1)))
                Stepper("Out \(model.loopEnd)", value: $model.loopEnd,
                        in: min(model.loopStart + 1, model.frameCount)...max(model.frameCount, 1))
            }
            .controlSize(.small)
            HStack(spacing: 8) {
                Button("+") {
                    withAnimation(Motion.panel.resolving(reduceMotion: reduceMotion)) {
                        model.addFrame(duplicate: false)
                    }
                }
                Button("Duplicate") {
                    withAnimation(Motion.panel.resolving(reduceMotion: reduceMotion)) {
                        model.addFrame(duplicate: true)
                    }
                }
                Button("−") {
                    withAnimation(Motion.panel.resolving(reduceMotion: reduceMotion)) {
                        model.removeFrame()
                    }
                }
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
            .animation(Motion.select.resolving(reduceMotion: reduceMotion), value: model.activeFrame)
        }
        .onAppear { model.refreshFrames() }
    }
}
