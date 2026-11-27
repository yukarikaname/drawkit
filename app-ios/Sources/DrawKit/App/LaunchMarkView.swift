// The iPad launch mark. The system launch screen shows the same tiles and stroke; this view draws
// them in, then fades out and lets the canvas take over.

import SwiftUI

struct LaunchMarkView: View {
    let reduceMotion: Bool
    @State private var revealed = false
    @State private var stroke: CGFloat = 0

    var body: some View {
        ZStack {
            Brand.ink.ignoresSafeArea()
            VStack(spacing: 20) {
                ZStack {
                    RoundedRectangle(cornerRadius: 16, style: .continuous)
                        .stroke(Brand.paper.opacity(0.55), lineWidth: 5)
                        .frame(width: 72, height: 72)
                        .offset(x: -18, y: -18)
                        .opacity(revealed ? 1 : 0)
                        .scaleEffect(revealed ? 1 : 0.62)
                    RoundedRectangle(cornerRadius: 16, style: .continuous)
                        .fill(Brand.vermilion)
                        .frame(width: 72, height: 72)
                        .offset(x: 18, y: 18)
                        .opacity(revealed ? 1 : 0)
                        .scaleEffect(revealed ? 1 : 0.62)
                    Path { path in
                        path.move(to: CGPoint(x: 18, y: 102))
                        path.addQuadCurve(
                            to: CGPoint(x: 102, y: 18),
                            control: CGPoint(x: 66, y: 66))
                    }
                    .trim(from: 0, to: stroke)
                    .stroke(Brand.paper, style: StrokeStyle(lineWidth: 8, lineCap: .round))
                }
                .frame(width: 120, height: 120)

                VStack(spacing: 5) {
                    Text("DrawKit")
                        .font(.system(size: 30, weight: .bold, design: .rounded))
                        .foregroundStyle(Brand.paper)
                    Text("PAINT WHAT YOU SEE")
                        .font(.system(size: 10, weight: .semibold, design: .rounded))
                        .tracking(1.8)
                        .foregroundStyle(Brand.paper.opacity(0.42))
                }
                .opacity(revealed ? 1 : 0)
                .offset(y: revealed ? 0 : 8)
            }
        }
        .accessibilityHidden(true)
        .onAppear {
            guard !reduceMotion else {
                revealed = true
                stroke = 1
                return
            }
            withAnimation(.spring(response: 0.5, dampingFraction: 0.78)) {
                revealed = true
            }
            withAnimation(.easeInOut(duration: 0.62).delay(0.18)) {
                stroke = 1
            }
        }
    }
}
