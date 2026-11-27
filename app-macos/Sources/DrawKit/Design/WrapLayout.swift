// A wrapping row: the tool options bar carries a different set of controls per tool and does not
// fit a narrow window. Letting it wrap keeps every control visible, where scrolling hid the ones
// off the right edge. Children are placed by hand, so the row has to honour the layout direction
// itself: the first child sits at the leading edge, which is the right edge in a right-to-left
// layout. The direction is passed in because a Layout cannot read the environment.

import SwiftUI

struct WrapLayout: Layout {
    var layoutDirection: LayoutDirection = .leftToRight
    var spacing: CGFloat = 14
    var lineSpacing: CGFloat = 8

    func sizeThatFits(proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) -> CGSize {
        let limit = proposal.width ?? .infinity
        var x: CGFloat = 0
        var y: CGFloat = 0
        var line: CGFloat = 0
        for subview in subviews {
            let size = fitted(subview, limit: limit)
            if x > 0 && x + size.width > limit {
                y += line + lineSpacing
                x = 0
                line = 0
            }
            x += size.width + spacing
            line = max(line, size.height)
        }
        let width = proposal.width ?? max(0, x - spacing)
        return CGSize(width: width, height: y + line)
    }

    func placeSubviews(in bounds: CGRect, proposal: ProposedViewSize,
                       subviews: Subviews, cache: inout ()) {
        var x = bounds.minX
        var y = bounds.minY
        var line: CGFloat = 0
        let mirrored = layoutDirection == .rightToLeft
        for subview in subviews {
            let size = fitted(subview, limit: bounds.width)
            if x > bounds.minX && x + size.width > bounds.maxX {
                y += line + lineSpacing
                x = bounds.minX
                line = 0
            }
            // x is a leading-edge cursor. In RTL the same slot is measured back from the right
            // edge, but the child is still placed from its physical top-left corner.
            let originX = mirrored ? bounds.maxX - (x - bounds.minX) - size.width : x
            subview.place(at: CGPoint(x: originX, y: y), anchor: UnitPoint(x: 0, y: 0),
                          proposal: ProposedViewSize(size))
            x += size.width + spacing
            line = max(line, size.height)
        }
    }

    /// An item wider than the whole line is measured (and later placed) against the line instead
    /// of its ideal width, so a long hint text wraps inside itself rather than overhanging.
    private func fitted(_ subview: LayoutSubview, limit: CGFloat) -> CGSize {
        let ideal = subview.sizeThatFits(.unspecified)
        guard ideal.width > limit, limit.isFinite, limit > 0 else { return ideal }
        return subview.sizeThatFits(ProposedViewSize(width: limit, height: nil))
    }
}
