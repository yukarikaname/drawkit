// The first-run setup: pressure, shortcuts, ready. It can be reopened from Settings, and the
// welcome screen waits underneath once the setup is done.

import SwiftUI
import AppKit

struct OnboardingView: View {
    @Environment(AppSettings.self) private var settings
    let shortcuts: ShortcutStore
    let onFinish: () -> Void

    @State private var page = 0
    @State private var measuredLow = 0.0
    @State private var measuredHigh = 1.0
    @State private var hasMeasured = false
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    private let pageCount = 3

    var body: some View {
        ZStack {
            Brand.ink
            VStack(spacing: 0) {
                header
                Spacer()
                card
                Spacer()
                controls
            }
            .padding(38)
        }
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
                ForEach(AppLanguage.allCases) { language in
                    Text(language.title).tag(language)
                }
            }
            .labelsHidden()
            .pickerStyle(.menu)
            .frame(width: 130)
        }
    }

    private var card: some View {
        VStack(spacing: 16) {
            switch page {
            case 0: pressurePage
            case 1: shortcutsPage
            default: readyPage
            }
            HStack(spacing: 7) {
                ForEach(0..<pageCount, id: \.self) { index in
                    Capsule()
                        .fill(index == page ? Brand.vermilion : Brand.paper.opacity(0.18))
                        .frame(width: index == page ? 18 : 6, height: 6)
                        .animation(.spring(response: 0.3, dampingFraction: 0.8), value: page)
                }
            }
        }
        .padding(.horizontal, 32)
        .padding(.vertical, 30)
        .frame(maxWidth: 640)
        .background(Brand.carbon.opacity(0.9), in: RoundedRectangle(cornerRadius: 24))
        .overlay(RoundedRectangle(cornerRadius: 24)
            .stroke(Color.white.opacity(0.07), lineWidth: 1))
        .shadow(color: .black.opacity(0.35), radius: 24, y: 10)
    }

    private var pressurePage: some View {
        VStack(spacing: 12) {
            Image(systemName: "pencil.tip")
                .font(.system(size: 32, weight: .light))
                .foregroundStyle(Brand.vermilion)
                .frame(height: 36)
            Text(settings.t(.onboardingPressureTitle))
                .font(.system(size: 25, weight: .bold, design: .rounded))
                .foregroundStyle(Brand.paper)
                .multilineTextAlignment(.center)
            Text(settings.t(.onboardingPressureBody))
                .font(.system(size: 13))
                .foregroundStyle(Brand.paper.opacity(0.68))
                .multilineTextAlignment(.center)
                .frame(maxWidth: 470)

            HStack(spacing: 10) {
                Text(settings.t(.onboardingSensitivity))
                    .font(.system(size: 12))
                    .foregroundStyle(Brand.muted)
                PenSlider(value: Binding(
                    get: { settings.pressureCurve },
                    set: { settings.pressureCurve = min(max($0, 0.5), 2.0) }
                ), range: 0.5...2.0, width: 220)
                Text(String(format: "%.2f", settings.pressureCurve))
                    .font(.system(size: 12).monospacedDigit())
                    .foregroundStyle(Brand.paper.opacity(0.8))
                    .frame(width: 34, alignment: .trailing)
            }
            .frame(maxWidth: 420)

            PressurePad(curve: settings.pressureCurve,
                        low: settings.pressureLow, high: settings.pressureHigh) { low, high in
                measuredLow = low
                measuredHigh = high
                hasMeasured = true
            }
            .frame(height: 140)
            .clipShape(RoundedRectangle(cornerRadius: 12))
            .overlay(RoundedRectangle(cornerRadius: 12)
                .stroke(Color.black.opacity(0.08), lineWidth: 1))

            HStack(spacing: 10) {
                Text(String(format: "measured %.2f – %.2f", measuredLow, measuredHigh))
                    .font(.system(size: 11).monospacedDigit())
                    .foregroundStyle(Brand.muted)
                Spacer()
                Button("Use this range") {
                    settings.pressureLow = measuredLow
                    settings.pressureHigh = measuredHigh
                }
                .buttonStyle(.bordered)
                .controlSize(.small)
                .disabled(!hasMeasured || measuredHigh - measuredLow < 0.05)
                Button("Reset") {
                    settings.pressureLow = 0
                    settings.pressureHigh = 1
                }
                .buttonStyle(.borderless)
                .foregroundStyle(Brand.muted)
            }
            .frame(maxWidth: 480)

            Text(settings.t(.onboardingTestHint))
                .font(.system(size: 11))
                .foregroundStyle(Brand.muted)
                .multilineTextAlignment(.center)
        }
    }

    private var shortcutsPage: some View {
        VStack(spacing: 14) {
            Image(systemName: "keyboard")
                .font(.system(size: 32, weight: .light))
                .foregroundStyle(Brand.vermilion)
                .frame(height: 36)
            Text(settings.t(.onboardingShortcutsTitle))
                .font(.system(size: 25, weight: .bold, design: .rounded))
                .foregroundStyle(Brand.paper)
                .multilineTextAlignment(.center)
            Text(settings.t(.onboardingShortcutsBody))
                .font(.system(size: 13))
                .foregroundStyle(Brand.paper.opacity(0.68))
                .multilineTextAlignment(.center)
                .frame(maxWidth: 470)
            LazyVGrid(columns: [GridItem(.flexible()), GridItem(.flexible())], spacing: 8) {
                ForEach(ShortcutPreset.allCases) { preset in
                    Button {
                        shortcuts.selectPreset(preset)
                    } label: {
                        HStack {
                            Text(preset.title)
                                .font(.system(size: 13, weight: .medium))
                            Spacer()
                            if shortcuts.preset == preset {
                                Image(systemName: "checkmark")
                                    .font(.system(size: 11, weight: .bold))
                            }
                        }
                        .foregroundStyle(Brand.paper)
                        .padding(.horizontal, 12)
                        .padding(.vertical, 10)
                        .background(
                            RoundedRectangle(cornerRadius: 10)
                                .fill(shortcuts.preset == preset
                                      ? Brand.vermilion.opacity(0.25)
                                      : Color.white.opacity(0.05))
                        )
                    }
                    .buttonStyle(.plain)
                }
            }
            .frame(maxWidth: 430)
            Text(settings.t(.onboardingShortcutsHelp))
                .font(.system(size: 11))
                .foregroundStyle(Brand.muted)
        }
    }

    private var readyPage: some View {
        VStack(spacing: 16) {
            BrandMark(size: 64, line: Brand.paper)
            Text(settings.t(.onboardingReadyTitle))
                .font(.system(size: 25, weight: .bold, design: .rounded))
                .foregroundStyle(Brand.paper)
            Text(settings.t(.onboardingReadyBody))
                .font(.system(size: 13))
                .foregroundStyle(Brand.paper.opacity(0.68))
                .multilineTextAlignment(.center)
                .frame(maxWidth: 460)
                .lineSpacing(3)
            Text("\(settings.t(.onboardingSensitivity)) \(String(format: "%.2f", settings.pressureCurve)) · \(shortcuts.preset.title)")
                .font(.system(size: 11).monospacedDigit())
                .foregroundStyle(Brand.muted)
        }
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
            Button(page == pageCount - 1
                   ? settings.t(.onboardingStart)
                   : settings.t(.onboardingNext)) {
                if page == pageCount - 1 {
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

/// A small strip where the pen draws with the real pressure curve. It is not the document; it is a
/// test surface, so the stroke can be thrown away when the setup closes.
private struct PressurePad: NSViewRepresentable {
    let curve: Double
    let low: Double
    let high: Double
    let onRange: (Double, Double) -> Void

    func makeNSView(context: Context) -> PressurePadView {
        let view = PressurePadView()
        view.curve = curve
        view.low = low
        view.high = high
        view.onRange = onRange
        return view
    }

    func updateNSView(_ view: PressurePadView, context: Context) {
        view.curve = curve
        view.low = low
        view.high = high
        view.onRange = onRange
    }
}

private final class PressurePadView: NSView {
    private struct Sample {
        let point: CGPoint
        let raw: CGFloat
    }

    var curve: Double = 1 {
        didSet { needsDisplay = true }
    }
    var low: Double = 0 {
        didSet { needsDisplay = true }
    }
    var high: Double = 1 {
        didSet { needsDisplay = true }
    }
    var onRange: ((Double, Double) -> Void)?

    private var strokes: [[Sample]] = []
    private var active: Int?
    private var minRaw: Double = 1
    private var maxRaw: Double = 0

    private func mapped(_ raw: Double) -> CGFloat {
        let span = max(high - low, 0.01)
        let value = min(max((raw - low) / span, 0.0), 1.0)
        return CGFloat(pow(value, curve))
    }

    override var isFlipped: Bool { true }

    override func draw(_ dirtyRect: NSRect) {
        let shape = NSBezierPath(roundedRect: bounds, xRadius: 12, yRadius: 12)
        (NSColor(Brand.paper)).setFill()
        shape.fill()
        shape.setClip()
        let ink = NSColor(Brand.ink)
        for stroke in strokes {
            guard let first = stroke.first else { continue }
            if stroke.count == 1 {
                let pressure = mapped(first.raw)
                let dot = NSRect(x: first.point.x - pressure * 4,
                                 y: first.point.y - pressure * 4,
                                 width: pressure * 8, height: pressure * 8)
                ink.setFill()
                NSBezierPath(ovalIn: dot).fill()
                continue
            }
            for index in 1..<stroke.count {
                let a = stroke[index - 1]
                let b = stroke[index]
                let path = NSBezierPath()
                path.move(to: a.point)
                path.line(to: b.point)
                path.lineCapStyle = .round
                path.lineWidth = 1 + 9 * mapped(a.raw)
                ink.withAlphaComponent(0.82).setStroke()
                path.stroke()
            }
        }
        let reading = strokes.last?.last.map { mapped($0.raw) } ?? 0
        let attributes: [NSAttributedString.Key: Any] = [
            .font: NSFont.monospacedDigitSystemFont(ofSize: 10, weight: .regular),
            .foregroundColor: NSColor.black.withAlphaComponent(0.45),
        ]
        NSAttributedString(string: String(format: "pressure %.2f", reading), attributes: attributes)
            .draw(at: NSPoint(x: 12, y: bounds.height - 20))
    }

    override func mouseDown(with event: NSEvent) {
        strokes.append([])
        active = strokes.count - 1
        add(event)
    }

    override func mouseDragged(with event: NSEvent) {
        add(event)
    }

    override func mouseUp(with event: NSEvent) {
        active = nil
        if maxRaw > minRaw {
            onRange?(minRaw, maxRaw)
        }
    }

    private func add(_ event: NSEvent) {
        guard let index = active else { return }
        let point = convert(event.locationInWindow, from: nil)
        guard bounds.contains(point) else { return }
        let raw = Double(event.pressure > 0.01 ? event.pressure : 1.0)
        minRaw = min(minRaw, raw)
        maxRaw = max(maxRaw, raw)
        strokes[index].append(Sample(point: point, raw: raw))
        needsDisplay = true
    }
}
