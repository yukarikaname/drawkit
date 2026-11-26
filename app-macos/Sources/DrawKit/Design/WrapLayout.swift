// A left-to-right row that wraps instead of clipping. The tool options bar carries a different
// set of controls per tool and does not fit a narrow window; letting it wrap keeps every control
// visible, where scrolling hid the ones off the right edge.

import SwiftUI

struct WrapLayout: Layout {
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
        for subview in subviews {
            let size = fitted(subview, limit: bounds.width)
            if x > bounds.minX && x + size.width > bounds.maxX {
                y += line + lineSpacing
                x = bounds.minX
                line = 0
            }
            subview.place(at: CGPoint(x: x, y: y), anchor: .topLeading,
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
