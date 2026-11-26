// The brush presets the core knows. A preset changes hardness, flow and how much pressure changes
// the width; the shell only names them.

import SwiftUI

enum BrushKind: Int, CaseIterable, Identifiable {
    case softRound = 0
    case pencil = 1
    case ink = 2
    case marker = 3
    case airbrush = 4
    case bristle = 5
    case spray = 6
    case chalk = 7
    case watercolor = 8

    var id: Self { self }

    var title: LocalizedStringResource {
        switch self {
        case .softRound: "Soft round"
        case .pencil: "Pencil"
        case .ink: "Ink"
        case .marker: "Marker"
        case .airbrush: "Airbrush"
        case .bristle: "Bristle"
        case .spray: "Spray"
        case .chalk: "Chalk"
        case .watercolor: "Watercolor"
        }
    }

    var symbol: String {
        switch self {
        case .softRound: "circle.fill"
        case .pencil: "pencil"
        case .ink: "pencil.tip"
        case .marker: "highlighter"
        case .airbrush: "aqi.low"
        case .bristle: "paintbrush"
        case .spray: "aqi.medium"
        case .chalk: "scribble"
        case .watercolor: "drop"
        }
    }
}
