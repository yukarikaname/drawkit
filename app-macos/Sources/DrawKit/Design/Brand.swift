// The things that make DrawKit look like DrawKit and not like a system dialog: a warm ink ground,
// one vermilion accent, and a mark made of two tiles and a brush stroke.
//
// The accent is only used on surfaces we draw ourselves - the tool rail, the mark, the empty canvas.
// Native controls keep the system accent the person chose in Settings.

import SwiftUI

enum Brand {
    static let vermilion = Color(.sRGB, red: 0.886, green: 0.337, blue: 0.243, opacity: 1)
    static let vermilionDeep = Color(.sRGB, red: 0.780, green: 0.243, blue: 0.165, opacity: 1)
    static let ink = Color(.sRGB, red: 0.106, green: 0.098, blue: 0.090, opacity: 1)
    static let carbon = Color(.sRGB, red: 0.149, green: 0.137, blue: 0.125, opacity: 1)
    static let raised = Color(.sRGB, red: 0.200, green: 0.184, blue: 0.169, opacity: 1)
    static let paper = Color(.sRGB, red: 0.965, green: 0.945, blue: 0.914, opacity: 1)
    static let muted = Color(.sRGB, red: 0.612, green: 0.580, blue: 0.541, opacity: 1)
}

/// Two offset tiles and a diagonal stroke: a kit of layers, and the line that joins them.
struct BrandMark: View {
    var size: CGFloat = 22
    var line: Color = .primary

    var body: some View {
        ZStack {
            RoundedRectangle(cornerRadius: size * 0.14)
                .stroke(line.opacity(0.55), lineWidth: size * 0.07)
                .frame(width: size * 0.52, height: size * 0.52)
                .offset(x: -size * 0.15, y: -size * 0.15)
            RoundedRectangle(cornerRadius: size * 0.14)
                .fill(Brand.vermilion)
                .frame(width: size * 0.52, height: size * 0.52)
                .offset(x: size * 0.15, y: size * 0.15)
            Path { path in
                path.move(to: CGPoint(x: size * 0.20, y: size * 0.80))
                path.addQuadCurve(
                    to: CGPoint(x: size * 0.80, y: size * 0.20),
                    control: CGPoint(x: size * 0.56, y: size * 0.56))
            }
            .stroke(Brand.paper, style: StrokeStyle(lineWidth: size * 0.085, lineCap: .round))
        }
        .frame(width: size, height: size)
        .accessibilityHidden(true)
    }
}
