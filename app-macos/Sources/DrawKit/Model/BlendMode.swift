// The blend modes the compositor understands. Kept next to the layer palette; the core owns the
// actual pixel maths.

import SwiftUI

enum BlendMode: Int, CaseIterable, Identifiable {
    case normal = 0
    case multiply = 1
    case screen = 2
    case overlay = 3
    case add = 4
    case darken = 5
    case lighten = 6
    case color = 7
    case luminosity = 8

    var id: Self { self }

    var title: String {
        switch self {
        case .normal: "Normal"
        case .multiply: "Multiply"
        case .screen: "Screen"
        case .overlay: "Overlay"
        case .add: "Add"
        case .darken: "Darken"
        case .lighten: "Lighten"
        case .color: "Color"
        case .luminosity: "Luminosity"
        }
    }
}
