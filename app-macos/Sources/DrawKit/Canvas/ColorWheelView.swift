// The painter's spectrum: a hue wheel above a saturation/value square.
//
// The images come from the Rust core, so the macOS view and the GPUI prototype share the picker
// geometry. The view is only the two controls; the panel around it owns the title, the tabs and the
// recent colours.

import SwiftUI
import AppKit

struct ColorWheelView: View {
    @Binding var color: Color
    var colorSpace: PaintColorSpace = .sRGB

    @State private var hue: CGFloat = 0
    @State private var saturation: CGFloat = 0
    @State private var brightness: CGFloat = 0.08
    @State private var wheel: CGImage?
    @State private var sv: CGImage?
    @State private var updating = false
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    private let wheelSize = 256

    var body: some View {
        VStack(spacing: 12) {
            GeometryReader { geometry in
                ZStack {
                    if let wheel {
                        image(wheel)
                    }
                    handle
                        .position(wheelPoint(in: geometry.size))
                        .animation(Motion.handle.resolving(reduceMotion: reduceMotion),
                                   value: hue)
                }
                .contentShape(Circle())
                .gesture(DragGesture(minimumDistance: 0)
                    .onChanged { value in setHue(at: value.location, in: geometry.size) })
            }
            .aspectRatio(1, contentMode: .fit)

            GeometryReader { geometry in
                ZStack {
                    if let sv {
                        image(sv)
                    }
                    handle
                        .position(svPoint(in: geometry.size))
                        .animation(Motion.handle.resolving(reduceMotion: reduceMotion),
                                   value: saturation)
                        .animation(Motion.handle.resolving(reduceMotion: reduceMotion),
                                   value: brightness)
                }
                .contentShape(Rectangle())
                .gesture(DragGesture(minimumDistance: 0)
                    .onChanged { value in setSV(at: value.location, in: geometry.size) })
            }
            .frame(height: 120)
            .clipShape(RoundedRectangle(cornerRadius: 8))
        }
        .onAppear {
            syncFromColor()
            loadWheel()
            loadSV()
        }
        .onChange(of: color) { _, _ in
            guard !updating else { return }
            syncFromColor()
            loadSV()
        }
    }

    private var handle: some View {
        Circle()
            .stroke(Color.white, lineWidth: 2)
            .background(Circle().stroke(Color.black.opacity(0.6), lineWidth: 1))
            .frame(width: 14, height: 14)
            .shadow(color: .black.opacity(0.35), radius: 2)
    }

    private func image(_ image: CGImage) -> some View {
        Image(decorative: image, scale: 1)
            .resizable()
            .interpolation(.high)
    }

    private func wheelPoint(in size: CGSize) -> CGPoint {
        let radius = min(size.width, size.height) / 2 - 2
        let centre = CGPoint(x: size.width / 2, y: size.height / 2)
        return CGPoint(x: centre.x + radius * cos(hue * .pi * 2),
                       y: centre.y + radius * sin(hue * .pi * 2))
    }

    private func svPoint(in size: CGSize) -> CGPoint {
        CGPoint(x: saturation * size.width,
                y: (1 - brightness) * size.height)
    }

    private func setHue(at point: CGPoint, in size: CGSize) {
        let centre = CGPoint(x: size.width / 2, y: size.height / 2)
        let angle = atan2(point.y - centre.y, point.x - centre.x) / (.pi * 2)
        hue = angle < 0 ? angle + 1 : angle
        applyCurrent()
        loadSV()
    }

    private func setSV(at point: CGPoint, in size: CGSize) {
        guard size.width > 0, size.height > 0 else { return }
        saturation = min(max(point.x / size.width, 0), 1)
        brightness = min(max(1 - point.y / size.height, 0), 1)
        applyCurrent()
    }

    private func applyCurrent() {
        updating = true
        color = Color(hue: Double(hue), saturation: Double(saturation),
                      brightness: Double(brightness), opacity: 1)
        updating = false
    }

    private func syncFromColor() {
        guard let ns = NSColor(color).usingColorSpace(.deviceRGB) else { return }
        var h: CGFloat = 0
        var s: CGFloat = 0
        var b: CGFloat = 0
        var a: CGFloat = 1
        ns.getHue(&h, saturation: &s, brightness: &b, alpha: &a)
        hue = h
        saturation = s
        brightness = b
    }

    private func loadWheel() {
        guard wheel == nil else { return }
        var bytes = [UInt8](repeating: 0, count: wheelSize * wheelSize * 4)
        let drawn = bytes.withUnsafeMutableBytes {
            dk_picker_wheel(UInt32(wheelSize), $0.bindMemory(to: UInt8.self).baseAddress)
        }
        guard drawn != 0 else { return }
        wheel = makeImage(bytes, wheelSize, wheelSize)
    }

    private func loadSV() {
        let width = 256
        let height = 120
        var bytes = [UInt8](repeating: 0, count: width * height * 4)
        let drawn = bytes.withUnsafeMutableBytes {
            dk_picker_sv(Float(hue), UInt32(width), UInt32(height),
                         $0.bindMemory(to: UInt8.self).baseAddress)
        }
        guard drawn != 0 else { return }
        sv = makeImage(bytes, width, height)
    }

    private func makeImage(_ bytes: [UInt8], _ width: Int, _ height: Int) -> CGImage? {
        guard let provider = CGDataProvider(data: Data(bytes) as CFData) else { return nil }
        return CGImage(
            width: width, height: height, bitsPerComponent: 8, bitsPerPixel: 32,
            bytesPerRow: width * 4, space: colorSpace.cgColorSpace,
            bitmapInfo: CGBitmapInfo(rawValue: CGImageAlphaInfo.premultipliedLast.rawValue),
            provider: provider, decode: nil, shouldInterpolate: true, intent: .defaultIntent)
    }
}
