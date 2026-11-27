// A build can be checked in a right-to-left layout before an RTL translation exists:
// `DRAWKIT_LAYOUT_DIRECTION=rtl ./dist/DrawKit.app/Contents/MacOS/DrawKit` forces the whole
// window to lay out right-to-left, and `ltr` forces the other way. With the variable unset the
// system direction wins, so a real RTL locale behaves normally. Read once at launch.

import SwiftUI

enum LayoutDirectionOverride {
    static let direction: LayoutDirection? = {
        let value = ProcessInfo.processInfo.environment["DRAWKIT_LAYOUT_DIRECTION"]?
            .trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
        switch value {
        case "rtl", "right-to-left": return .rightToLeft
        case "ltr", "left-to-right": return .leftToRight
        default: return nil
        }
    }()
}

private struct LayoutDirectionOverrideModifier: ViewModifier {
    @Environment(\.layoutDirection) private var inherited

    func body(content: Content) -> some View {
        content.environment(\.layoutDirection, LayoutDirectionOverride.direction ?? inherited)
    }
}

extension View {
    /// Applies the debug direction when one is set; otherwise re-applies the inherited direction.
    func drawKitLayoutDirection() -> some View {
        modifier(LayoutDirectionOverrideModifier())
    }
}
