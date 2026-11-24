// The four tablet tools, their symbols and their hardware-keyboard letters.

import SwiftUI

enum IOSTool: String, CaseIterable, Identifiable {
    case brush = "Brush"
    case eraser = "Eraser"
    case fill = "Fill"
    case eyedropper = "Eyedropper"
    case select = "Select"

    var id: Self { self }

    var symbol: String {
        switch self {
        case .brush: "paintbrush.pointed.fill"
        case .eraser: "eraser.fill"
        case .fill: "drop.fill"
        case .eyedropper: "eyedropper"
        case .select: "rectangle.dashed"
        }
    }

    /// Hardware-keyboard shortcut, the same letter the desktop uses.
    var shortcut: KeyEquivalent {
        switch self {
        case .brush: "b"
        case .eraser: "e"
        case .fill: "f"
        case .eyedropper: "i"
        case .select: "m"
        }
    }
}
