// Motion tokens: every state change settles, nothing snaps.
//
// The canvas is exempt - a brush stroke must stay 1:1 with the pointer. Chrome, on the other hand,
// uses a spring so a tool, a palette row or a replay bar arrives with weight rather than appearing.
// Reduce Motion degrades every token to a short fade instead of a longer journey.

import SwiftUI

enum Motion {
    case select
    case panel
    case press
    case settle
    case handle

    var base: Animation {
        switch self {
        case .select: .spring(response: 0.26, dampingFraction: 0.74)
        case .panel: .spring(response: 0.38, dampingFraction: 0.86)
        case .press: .easeOut(duration: 0.12)
        case .settle: .spring(response: 0.45, dampingFraction: 0.80)
        case .handle: .spring(response: 0.16, dampingFraction: 0.90)
        }
    }

    /// The token, or a fade when the person asked for less movement.
    func resolving(reduceMotion: Bool) -> Animation {
        reduceMotion ? .easeOut(duration: 0.10) : base
    }
}

/// A custom-drawn button answers the press with a short scale, never a linear ramp. System
/// controls keep their own feedback.
struct SpringPressStyle: ButtonStyle {
    let reduceMotion: Bool

    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .scaleEffect(configuration.isPressed ? 0.94 : 1)
            .animation(Motion.press.resolving(reduceMotion: reduceMotion),
                       value: configuration.isPressed)
    }
}
