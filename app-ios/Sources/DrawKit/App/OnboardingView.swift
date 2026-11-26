// The first-run tour on the iPad. Same four pages as macOS, over the canvas; the settings sheet can
// open it again.

import SwiftUI

struct OnboardingView: View {
    @Environment(AppSettings.self) private var settings
    let onFinish: () -> Void

    @State private var page = 0
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    private var pages: [(String, LocalizedStringKey, LocalizedStringKey)] {
        [
            ("paintbrush.pointed.fill", "One canvas, every size",
             "Choose the paper before you paint. A project or image opens at its own size, and the canvas can be larger than the screen."),
            ("drop.fill", "Tools that reach the edge",
             "Gap closing, growth and softening make a fill meet the line instead of leaving a pale rim."),
            ("pencil.tip", "A pen, not a mouse",
             "Pencil and tablet pressure, tilt and rotation reach the stroke. Mirror can paint both sides at once."),
            ("square.3.layers.3d", "Strokes are the document",
             "The tape keeps the brush, seed and path, so a painting can be re-rendered at another size. Pixels are its cache."),
        ]
    }

    var body: some View {
        ZStack {
            Brand.ink.ignoresSafeArea()
            VStack(spacing: 0) {
                header
                Spacer()
                card
                Spacer()
                controls
            }
            .padding(28)
        }
        .preferredColorScheme(.dark)
    }

    private var header: some View {
        HStack(spacing: 10) {
            BrandMark(size: 26, line: Brand.paper)
            Text("DrawKit")
                .font(.system(size: 17, weight: .semibold, design: .rounded))
                .foregroundStyle(Brand.paper)
            Spacer()
        }
    }

    private var card: some View {
        VStack(spacing: 18) {
            if page == 0 {
                BrandMark(size: 64, line: Brand.paper)
            } else {
                Image(systemName: pages[page].0)
                    .font(.system(size: 42, weight: .light))
                    .foregroundStyle(Brand.vermilion)
                    .frame(height: 64)
            }
            Text(pages[page].1)
                .font(.system(size: 27, weight: .bold, design: .rounded))
                .foregroundStyle(Brand.paper)
                .multilineTextAlignment(.center)
            Text(pages[page].2)
                .font(.system(size: 14))
                .foregroundStyle(Brand.paper.opacity(0.68))
                .multilineTextAlignment(.center)
                .lineSpacing(3)
                .frame(maxWidth: 480)
            HStack(spacing: 7) {
                ForEach(pages.indices, id: \.self) { index in
                    Capsule()
                        .fill(index == page ? Brand.vermilion : Brand.paper.opacity(0.18))
                        .frame(width: index == page ? 18 : 6, height: 6)
                        .animation(.spring(response: 0.3, dampingFraction: 0.8), value: page)
                }
            }
        }
        .padding(.horizontal, 30)
        .padding(.vertical, 32)
        .frame(maxWidth: 620)
        .background(Brand.carbon.opacity(0.88), in: RoundedRectangle(cornerRadius: 24))
        .overlay(RoundedRectangle(cornerRadius: 24)
            .stroke(Color.white.opacity(0.07), lineWidth: 1))
        .shadow(color: .black.opacity(0.35), radius: 24, y: 10)
    }

    private var controls: some View {
        HStack(spacing: 12) {
            Button("Skip") { onFinish() }
                .buttonStyle(.borderless)
                .foregroundStyle(Brand.muted)
            Spacer()
            if page > 0 {
                Button("Back") {
                    withAnimation(stepAnimation) { page -= 1 }
                }
                .buttonStyle(.bordered)
            }
            Button(page == pages.count - 1
                   ? "Start painting"
                   : "Next") {
                if page == pages.count - 1 {
                    onFinish()
                } else {
                    withAnimation(stepAnimation) { page += 1 }
                }
            }
            .buttonStyle(.borderedProminent)
            .tint(Brand.vermilion)
            .controlSize(.large)
        }
    }

    private var stepAnimation: Animation? {
        reduceMotion ? .easeOut(duration: 0.12) : .spring(response: 0.32, dampingFraction: 0.82)
    }
}
