// The brush presets the core knows. A preset changes hardness, flow and how much pressure changes
// the width; the shell only names them.

import SwiftUI

enum BrushKind: Int, CaseIterable, Identifiable {
    case softRound = 0
    case pencil = 1
    case ink = 2
    case marker = 3
    case airbrush = 4

    var id: Self { self }

    var title: String {
        switch self {
        case .softRound: "Soft round"
        case .pencil: "Pencil"
        case .ink: "Ink"
        case .marker: "Marker"
        case .airbrush: "Airbrush"
        }
    }

    var symbol: String {
        switch self {
        case .softRound: "circle.fill"
        case .pencil: "pencil"
        case .ink: "pencil.tip"
        case .marker: "highlighter"
        case .airbrush: "aqi.low"
        }
    }
}
