// The things that make DrawKit look like DrawKit and not like a system dialog: a warm ink ground,
// one vermilion accent, and a mark made of two tiles and a brush stroke.
//
// The accent is only used on surfaces we draw ourselves - the tool rail, the mark, the empty canvas.
// Native controls keep the system accent the person chose in Settings.

import SwiftUI

enum Brand {
    static let graphite = Color(.sRGB, red: 0.078, green: 0.090, blue: 0.102, opacity: 1)
    static let slate = Color(.sRGB, red: 0.114, green: 0.129, blue: 0.149, opacity: 1)
    static let rule = Color(.sRGB, red: 0.180, green: 0.204, blue: 0.227, opacity: 1)
    static let chalk = Color(.sRGB, red: 0.925, green: 0.918, blue: 0.894, opacity: 1)
    static let ash = Color(.sRGB, red: 0.553, green: 0.584, blue: 0.616, opacity: 1)
    static let proof = Color(.sRGB, red: 0.957, green: 0.945, blue: 0.918, opacity: 1)
    static let rust = Color(.sRGB, red: 0.706, green: 0.278, blue: 0.184, opacity: 1)

    // Names the shells still read while their chrome moves from cards to rules.
    static let vermilion = rust
    static let vermilionDeep = rust
    static let ink = graphite
    static let carbon = slate
    static let raised = rule
    static let paper = chalk
    static let muted = ash
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
                .fill(Brand.proof)
                .frame(width: size * 0.52, height: size * 0.52)
                .offset(x: size * 0.15, y: size * 0.15)
            Path { path in
                path.move(to: CGPoint(x: size * 0.20, y: size * 0.80))
                path.addQuadCurve(
                    to: CGPoint(x: size * 0.80, y: size * 0.20),
                    control: CGPoint(x: size * 0.56, y: size * 0.56))
            }
            .stroke(Brand.chalk, style: StrokeStyle(lineWidth: size * 0.085, lineCap: .round))
        }
        .frame(width: size, height: size)
        .accessibilityHidden(true)
    }
}
