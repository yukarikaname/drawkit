// The iPad animation sheet: frames, hold, loop and onion skin. The canvas stays visible behind
// the sheet's medium detent, so a frame can be judged while the strip is open.

import SwiftUI

struct AnimationSheet: View {
    @Bindable var studio: Studio
    @Environment(\.dismiss) private var dismiss

    private var startRange: ClosedRange<Int> {
        let lower = 0
        let upper = max(0, min(studio.loopEnd - 1, studio.frameCount - 1))
        return lower...upper
    }

    private var endRange: ClosedRange<Int> {
        let lower = min(studio.loopStart + 1, studio.frameCount)
        let upper = max(studio.frameCount, 1)
        return lower...upper
    }

    var body: some View {
        NavigationStack {
            VStack(spacing: 12) {
                ScrollView(.horizontal, showsIndicators: false) {
                    HStack(spacing: 6) {
                        ForEach(0..<studio.frameCount, id: \.self) { index in
                            Button {
                                studio.selectFrame(index)
                            } label: {
                                ZStack {
                                    RoundedRectangle(cornerRadius: 7, style: .continuous)
                                        .fill(Brand.ink)
                                    if index < studio.frameThumbnails.count,
                                       let image = studio.frameThumbnails[index] {
                                        Image(uiImage: image)
                                            .resizable()
                                            .aspectRatio(contentMode: .fit)
                                    }
                                }
                                .frame(width: 58, height: 58)
                                .overlay(
                                    RoundedRectangle(cornerRadius: 7, style: .continuous)
                                        .stroke(index == studio.activeFrame
                                                ? Brand.vermilion : Color.white.opacity(0.10),
                                                lineWidth: index == studio.activeFrame ? 2 : 1)
                                )
                            }
                            .buttonStyle(.plain)
                        }
                    }
                    .padding(.vertical, 2)
                }

                HStack(spacing: 10) {
                    Button { studio.stepFrame(-1) } label: {
                        Image(systemName: "backward.frame.fill")
                    }
                    Button { studio.togglePlayback() } label: {
                        Image(systemName: studio.isPlaying ? "pause.fill" : "play.fill")
                    }
                    Button { studio.stepFrame(1) } label: {
                        Image(systemName: "forward.frame.fill")
                    }
                    Stepper("FPS \(studio.fps)", value: $studio.fps, in: 1...60)
                    Stepper("Hold \(studio.frameDuration) ms",
                            value: $studio.frameDuration, in: 10...2_000, step: 10)
                }
                .buttonStyle(.bordered)
                .controlSize(.small)

                HStack(spacing: 10) {
                    Toggle("Loop", isOn: $studio.loopEnabled)
                    Stepper("In \(studio.loopStart + 1)", value: $studio.loopStart,
                            in: startRange)
                    Stepper("Out \(studio.loopEnd)", value: $studio.loopEnd,
                            in: endRange)
                    Spacer()
                    Button("+") { studio.addFrame(duplicate: false) }
                    Button("Duplicate") { studio.addFrame(duplicate: true) }
                    Button("−") { studio.removeFrame() }
                }
                .buttonStyle(.bordered)
                .controlSize(.small)

                HStack(spacing: 10) {
                    Toggle("Onion skin", isOn: $studio.onionEnabled)
                    Stepper("Before \(studio.onionBefore)", value: $studio.onionBefore, in: 0...3)
                    Stepper("After \(studio.onionAfter)", value: $studio.onionAfter, in: 0...3)
                    Spacer()
                }
                .controlSize(.small)

                Spacer(minLength: 0)
            }
            .padding(16)
            .navigationTitle("Animation")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .confirmationAction) {
                    Button("Done") { dismiss() }
                }
            }
            .preferredColorScheme(.dark)
            .onAppear { studio.refreshFrames() }
        }
    }
}
