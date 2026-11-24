// A radial menu: a ring of tool buttons with a smaller ring of colour swatches inside.
//
// Shared by the macOS and iPad shells; it knows nothing about a document. The shell builds the
// items, this draws and animates them. Only SwiftUI is used so both targets compile the same file.
//
// The look follows the app's studio idea: a dark ink platter with a soft sheen, one vermilion
// accent, tokens that sit on the platter instead of system buttons, and items that grow out from
// the centre rather than cross-fading into place.

import SwiftUI

struct RadialMenuItem: Identifiable {
    let id: String
    let title: String
    var systemImage: String?
    var color: Color?
    var selected: Bool = false
    let action: () -> Void

    init(id: String, title: String, systemImage: String? = nil, color: Color? = nil,
         selected: Bool = false, action: @escaping () -> Void) {
        self.id = id
        self.title = title
        self.systemImage = systemImage
        self.color = color
        self.selected = selected
        self.action = action
    }
}

struct RadialMenu: View {
    let title: String
    let systemImage: String
    let outer: [RadialMenuItem]
    let inner: [RadialMenuItem]
    var accent: Color = .accentColor
    var plate: Color = Color.black.opacity(0.78)
    var foreground: Color = .primary
    var visible: Bool = true
    let onDismiss: () -> Void

    @State private var shown = false
    @State private var hovered: RadialMenuItem?
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    private var animation: Animation {
        reduceMotion ? .easeOut(duration: 0.14) : .spring(response: 0.38, dampingFraction: 0.74)
    }

    var body: some View {
        ZStack {
            platter
            ForEach(Array(inner.enumerated()), id: \.element.id) { index, item in
                button(item, radius: 76, angle: angle(index, inner.count), size: 26, delay: index)
            }
            ForEach(Array(outer.enumerated()), id: \.element.id) { index, item in
                button(item, radius: 124, angle: angle(index, outer.count), size: 42,
                       delay: index + inner.count)
            }
            centre
        }
        .frame(width: 332, height: 332)
        .opacity(shown ? 1 : 0)
        .rotationEffect(.degrees(shown ? 0 : -8))
        .animation(animation, value: shown)
        .onAppear { shown = visible }
        .onChange(of: visible) { _, value in
            withAnimation(animation) { shown = value }
        }
    }

    private var platter: some View {
        ZStack {
            Circle()
                .fill(plate)
                .glassEffect(.regular, in: .circle)
                .overlay(
                    Circle().fill(
                        LinearGradient(colors: [.white.opacity(0.10), .clear],
                                       startPoint: .top, endPoint: .center)
                    )
                )
                .overlay(Circle().strokeBorder(.white.opacity(0.12), lineWidth: 1))
                .shadow(color: .black.opacity(0.5), radius: 30, y: 14)
            Circle()
                .strokeBorder(.white.opacity(0.06), lineWidth: 1)
                .frame(width: 196, height: 196)
            ForEach(0..<48, id: \.self) { tick in
                Capsule()
                    .fill(.white.opacity(tick % 4 == 0 ? 0.14 : 0.06))
                    .frame(width: 1.2, height: tick % 4 == 0 ? 6 : 3)
                    .offset(y: -154)
                    .rotationEffect(.degrees(Double(tick) * 7.5))
            }
        }
        .frame(width: 316, height: 316)
    }

    private var centre: some View {
        VStack(spacing: 7) {
            Image(systemName: hovered?.systemImage ?? systemImage)
                .font(.system(size: 24, weight: .medium))
                .foregroundStyle(foreground.opacity(0.9))
            Text(hovered?.title ?? title)
                .font(.caption.weight(.semibold))
                .foregroundStyle(foreground.opacity(0.72))
                .lineLimit(1)
                .frame(maxWidth: 92)
            Button {
                onDismiss()
            } label: {
                Image(systemName: "xmark")
                    .font(.system(size: 9, weight: .bold))
                    .foregroundStyle(foreground.opacity(0.7))
                    .frame(width: 22, height: 22)
                    .background(Circle().fill(.white.opacity(0.08)))
            }
            .buttonStyle(.plain)
        }
        .animation(.easeOut(duration: 0.12), value: hovered?.id)
    }

    private func angle(_ index: Int, _ count: Int) -> Double {
        guard count > 0 else { return 0 }
        return -90 + 360 * Double(index) / Double(count)
    }

    private func position(_ degrees: Double, radius: CGFloat) -> CGSize {
        let radians = degrees * .pi / 180
        return CGSize(width: cos(radians) * radius, height: sin(radians) * radius)
    }

    private func button(_ item: RadialMenuItem, radius: CGFloat, angle degrees: Double,
                        size: CGFloat, delay: Int) -> some View {
        let point = position(degrees, radius: radius)
        let isHovered = hovered?.id == item.id
        return Button {
            onDismiss()
            item.action()
        } label: {
            ZStack {
                if let color = item.color {
                    Circle()
                        .fill(color)
                        .overlay(
                            Circle().strokeBorder(
                                item.selected ? accent : .white.opacity(0.7),
                                lineWidth: item.selected ? 3 : 2)
                        )
                        .shadow(color: .black.opacity(0.35), radius: 4, y: 2)
                } else {
                    Circle()
                        .fill(
                            LinearGradient(
                                colors: item.selected
                                    ? [accent, accent.opacity(0.72)]
                                    : [.white.opacity(0.18), .white.opacity(0.05)],
                                startPoint: .top, endPoint: .bottom)
                        )
                        .overlay(
                            Circle().strokeBorder(
                                item.selected ? .white.opacity(0.55) : .white.opacity(0.16),
                                lineWidth: 1)
                        )
                        .shadow(color: .black.opacity(0.4), radius: 5, y: 3)
                }
                if let symbol = item.systemImage {
                    Image(systemName: symbol)
                        .font(.system(size: size * 0.4, weight: .medium))
                        .foregroundStyle(item.selected ? .white : foreground.opacity(0.88))
                }
            }
            .frame(width: size, height: size)
            .scaleEffect(isHovered ? 1.16 : (item.selected ? 1.06 : 1))
            .shadow(color: isHovered ? .black.opacity(0.3) : .clear, radius: 8)
        }
        .buttonStyle(.plain)
        .accessibilityLabel(Text(item.title))
        .onHover { inside in
            withAnimation(.easeOut(duration: 0.12)) {
                if inside {
                    hovered = item
                } else if hovered?.id == item.id {
                    hovered = nil
                }
            }
        }
        .offset(shown ? point : .zero)
        .opacity(shown ? 1 : 0)
        .animation(animation.delay(Double(delay) * 0.014), value: shown)
    }
}
