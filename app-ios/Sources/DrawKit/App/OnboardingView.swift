// The first-run tour on the iPad. Same four pages as macOS, over the canvas; the settings sheet can
// open it again.

import SwiftUI

struct OnboardingView: View {
    @Environment(AppSettings.self) private var settings
    let onFinish: () -> Void

    @State private var page = 0
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    private var pages: [(String, IOSKey, IOSKey)] {
        [
            ("paintbrush.pointed.fill", .onboarding1Title, .onboarding1Body),
            ("drop.fill", .onboarding2Title, .onboarding2Body),
            ("pencil.tip", .onboarding3Title, .onboarding3Body),
            ("square.3.layers.3d", .onboarding4Title, .onboarding4Body),
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
            Picker("", selection: Binding(
                get: { settings.language },
                set: { settings.language = $0 }
            )) {
                ForEach(IOSLanguage.allCases) { language in
                    Text(language.title).tag(language)
                }
            }
            .labelsHidden()
            .pickerStyle(.menu)
            .frame(width: 130)
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
            Text(settings.t(pages[page].1))
                .font(.system(size: 27, weight: .bold, design: .rounded))
                .foregroundStyle(Brand.paper)
                .multilineTextAlignment(.center)
            Text(settings.t(pages[page].2))
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
            Button(settings.t(.onboardingSkip)) { onFinish() }
                .buttonStyle(.borderless)
                .foregroundStyle(Brand.muted)
            Spacer()
            if page > 0 {
                Button(settings.t(.onboardingBack)) {
                    withAnimation(stepAnimation) { page -= 1 }
                }
                .buttonStyle(.bordered)
            }
            Button(page == pages.count - 1
                   ? settings.t(.onboardingStart)
                   : settings.t(.onboardingNext)) {
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
