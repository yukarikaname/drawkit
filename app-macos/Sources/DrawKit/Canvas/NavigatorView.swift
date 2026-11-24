// The whole painting in the corner of the canvas: a quiet overview on a carbon plate, not another
// panel. It follows the layer thumbnails' debounce, so it does not rebuild for every segment.

import SwiftUI

struct NavigatorView: View {
    let model: PaintModel
    var onNavigate: ((CGPoint) -> Void)? = nil

    var body: some View {
        Group {
            if let overview = model.overview {
                Image(decorative: overview, scale: 1)
                    .resizable()
                    .interpolation(.medium)
                    .aspectRatio(contentMode: .fit)
            } else {
                RoundedRectangle(cornerRadius: 8, style: .continuous)
                    .fill(Brand.raised.opacity(0.5))
            }
        }
        .frame(width: 172, height: 120)
        .background(
            RoundedRectangle(cornerRadius: 12, style: .continuous)
                .fill(Brand.carbon)
                .overlay(RoundedRectangle(cornerRadius: 12, style: .continuous)
                    .stroke(.white.opacity(0.08), lineWidth: 1))
        )
        .overlay(alignment: .bottomLeading) {
            Text("OVERVIEW")
                .font(.system(size: 8, weight: .semibold))
                .tracking(1.2)
                .foregroundStyle(Brand.muted)
                .padding(.horizontal, 7)
                .padding(.vertical, 4)
                .background(
                    Capsule().fill(Brand.ink.opacity(0.6))
                )
                .padding(7)
        }
        .clipShape(RoundedRectangle(cornerRadius: 12, style: .continuous))
        .shadow(color: .black.opacity(0.3), radius: 14, y: 6)
        .overlay {
            GeometryReader { proxy in
                Color.clear
                    .contentShape(Rectangle())
                    .gesture(DragGesture(minimumDistance: 0).onEnded { value in
                        guard onNavigate != nil, proxy.size.width > 0, proxy.size.height > 0
                        else { return }
                        onNavigate?(CGPoint(x: min(max(value.location.x / proxy.size.width, 0), 1),
                                            y: min(max(value.location.y / proxy.size.height, 0), 1)))
                    })
            }
        }
        .padding(.top, 12)
        .padding(.trailing, 12)
        .accessibilityLabel("Canvas overview")
    }
}
