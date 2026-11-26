// The whole painting in the corner of the canvas: a quiet overview on a carbon plate, not another
// panel. It follows the layer thumbnails' debounce, so it does not rebuild for every segment.

import SwiftUI
import UniformTypeIdentifiers

struct NavigatorView: View {
    let model: PaintModel
    var viewport: CGRect = CGRect(x: 0, y: 0, width: 1, height: 1)
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
        .onDrag {
            // A file URL is what Finder, Mail and chat windows accept as a real PNG.
            guard let data = model.documentPNGData() else { return NSItemProvider() }
            let url = FileManager.default.temporaryDirectory
                .appendingPathComponent("DrawKit.png")
            try? data.write(to: url)
            return NSItemProvider(contentsOf: url) ?? NSItemProvider()
        }
        .help("Drag the painting out as a PNG")
        .clipShape(RoundedRectangle(cornerRadius: 12, style: .continuous))
        .shadow(color: .black.opacity(0.3), radius: 14, y: 6)
        .overlay {
            GeometryReader { proxy in
                let image = fittedImageRect(in: proxy.size)
                let frame = CGRect(x: image.minX + viewport.minX * image.width,
                                   y: image.minY + viewport.minY * image.height,
                                   width: viewport.width * image.width,
                                   height: viewport.height * image.height)
                ZStack(alignment: .topLeading) {
                    if frame.width > 0.5, frame.height > 0.5 {
                        RoundedRectangle(cornerRadius: 2, style: .continuous)
                            .fill(Brand.vermilion.opacity(0.12))
                            .overlay(
                                RoundedRectangle(cornerRadius: 2, style: .continuous)
                                    .stroke(Brand.paper.opacity(0.9), lineWidth: 1.5)
                            )
                            .frame(width: frame.width, height: frame.height)
                            .offset(x: frame.minX, y: frame.minY)
                            .allowsHitTesting(false)
                    }
                    Color.clear
                        .contentShape(Rectangle())
                        .gesture(DragGesture(minimumDistance: 0).onEnded { value in
                            guard onNavigate != nil,
                                  proxy.size.width > 0, proxy.size.height > 0 else { return }
                            let x = min(max((value.location.x - image.minX) / max(image.width, 1), 0), 1)
                            let y = min(max((value.location.y - image.minY) / max(image.height, 1), 0), 1)
                            onNavigate?(CGPoint(x: x, y: y))
                        })
                }
            }
        }
        .padding(.top, 12)
        .padding(.trailing, 12)
        .accessibilityLabel("Canvas overview")
    }

    /// The image is aspect-fit inside the 172×120 plate; the viewport hint must map to the image,
    /// not to the letterboxed frame.
    private func fittedImageRect(in size: CGSize) -> CGRect {
        guard model.width > 0, model.height > 0, size.width > 0, size.height > 0 else {
            return .zero
        }
        let aspect = CGFloat(model.width) / CGFloat(model.height)
        if size.width / size.height > aspect {
            let height = size.height
            let width = height * aspect
            return CGRect(x: (size.width - width) / 2, y: 0, width: width, height: height)
        } else {
            let width = size.width
            let height = width / aspect
            return CGRect(x: 0, y: (size.height - height) / 2, width: width, height: height)
        }
    }
}
