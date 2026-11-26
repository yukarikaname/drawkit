// The shell side of the radial quick menu: it knows the tools, presets and recent colours.
// The ring itself lives in shared/swift so the iPad shell can use the same animation later.

import SwiftUI

final class QuickMenuState: ObservableObject {
    @Published var visible = true
}

struct QuickMenuView: View {
    let model: PaintModel
    @ObservedObject var state: QuickMenuState
    let tool: Tool
    let brush: BrushKind
    let recentColours: [Color]
    let onTool: (Tool) -> Void
    let onBrush: (BrushKind) -> Void
    let onColor: (Color) -> Void
    let onFit: () -> Void
    let onClose: () -> Void

    var body: some View {
        RadialMenu(
            title: String(localized: title),
            systemImage: systemImage,
            outer: outerItems,
            inner: colourItems,
            accent: Brand.vermilion,
            plate: Brand.ink.opacity(0.55),
            foreground: Brand.paper,
            visible: state.visible,
            onDismiss: onClose
        )
    }

    private var title: LocalizedStringResource {
        switch tool {
        case .brush: brush.title
        case .eraser: "Eraser"
        case .smudge: "Smudge"
        case .fill: "Fill"
        case .eyedropper: "Eyedropper"
        case .select: "Selection"
        case .figure: "Figure"
        }
    }

    private var systemImage: String {
        switch tool {
        case .brush: brush.symbol
        case .eraser: "eraser"
        case .smudge: "drop.halffull"
        case .fill: "paintpalette.fill"
        case .eyedropper: "eyedropper"
        case .select: "rectangle.dashed"
        case .figure: "figure.stand"
        }
    }

    private var outerItems: [RadialMenuItem] {
        var items: [RadialMenuItem] = BrushKind.allCases.map { kind in
            RadialMenuItem(id: "brush-\(kind.rawValue)", title: String(localized: kind.title),
                           systemImage: kind.symbol,
                           selected: tool == .brush && brush == kind) {
                onBrush(kind)
                onTool(.brush)
            }
        }
        items.append(RadialMenuItem(id: "eraser", title: "Eraser", systemImage: "eraser",
                                    selected: tool == .eraser) { onTool(.eraser) })
        items.append(RadialMenuItem(id: "smudge", title: "Smudge", systemImage: "drop.halffull",
                                    selected: tool == .smudge) { onTool(.smudge) })
        items.append(RadialMenuItem(id: "fill", title: "Fill", systemImage: "paintpalette.fill",
                                    selected: tool == .fill) { onTool(.fill) })
        items.append(RadialMenuItem(id: "eyedropper", title: "Eyedropper", systemImage: "eyedropper",
                                    selected: tool == .eyedropper) { onTool(.eyedropper) })
        items.append(RadialMenuItem(id: "fit", title: "Fit canvas",
                                    systemImage: "arrow.up.left.and.arrow.down.right") {
            onFit()
        })
        return items
    }

    private var colourItems: [RadialMenuItem] {
        Array(recentColours.prefix(8)).enumerated().map { index, colour in
            RadialMenuItem(id: "colour-\(index)", title: "Colour \(index + 1)", color: colour) {
                onColor(colour)
            }
        }
    }
}


/// The ring is drawn over the canvas itself, centred on the pen, instead of inside an NSPopover:
/// the menu should grow out of the nib, not out of a little window with an arrow.
struct QuickMenuOverlay: View {
    let menu: QuickMenuView
    @ObservedObject var state: QuickMenuState
    let point: CGPoint
    let bounds: CGSize
    let onClose: () -> Void

    private var centre: CGPoint {
        let radius: CGFloat = 168
        let x = bounds.width < radius * 2 ? bounds.width / 2
            : min(max(point.x, radius), bounds.width - radius)
        let y = bounds.height < radius * 2 ? bounds.height / 2
            : min(max(point.y, radius), bounds.height - radius)
        return CGPoint(x: x, y: y)
    }

    var body: some View {
        ZStack(alignment: .topLeading) {
            Color.black.opacity(state.visible ? 0.18 : 0)
                .ignoresSafeArea()
                .contentShape(Rectangle())
                .onTapGesture { onClose() }
            menu
                .position(centre)
        }
        .animation(.easeOut(duration: 0.18), value: state.visible)
    }
}
