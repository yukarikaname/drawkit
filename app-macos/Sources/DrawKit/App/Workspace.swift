// The settings that belong to the window rather than to the document: the selected tool, colour,
// brush size, stabiliser, fill tolerance and zoom. The menu bar and the canvas both edit this one
// object, which is what makes a Tools menu possible without a second source of truth.

import SwiftUI
import Observation

@MainActor
@Observable
final class Workspace {
    var tool: Tool = .brush
    var color: Color = .black
    var size: Double = 8
    var stabilization: Double = 0
    var brush: BrushKind = .softRound
    var tolerance: Double = 0.12
    var fillClose: Double = 1
    var fillExpand: Double = 1
    var fillFeather: Double = 0
    /// Keep a refill close to the colour already in the region.
    var fillSnap = true
    /// How much paint a smudge drag carries, 0...1.
    var smudgeStrength: Double = 0.6
    /// Drop samples that repeat the previous one while the pen is stationary.
    var strokeDedupe = true
    /// Paint a short extrapolated segment ahead of the pen.
    var prediction = true
    /// Replace a finished stroke with the shape the tiny on-device model recognises.
    var autoShape = false
    /// A held stroke snaps even when Auto Shape is off.
    var quickShape = true
    var zoom: Double = 1
    /// Canvas rotation in degrees, clockwise on screen.
    var rotation: Double = 0
    /// Bumped by the View menu to ask the canvas to turn back upright.
    var rotationRequest = 0
    /// Canvas pan in view points, restored with the session.
    var panX: Double = 0
    var panY: Double = 0
    var mirrorEnabled = false
    /// Symmetry axis in degrees, 0 horizontal and 90 vertical.
    var mirrorAngle: Double = 90
    /// Also mirror across the perpendicular axis.
    var mirrorBoth = false
    /// Extra stops above paper white for HDR documents, 0...4.
    var glow: Double = 0
    /// Display exposure for an HDR preview, in stops.
    var exposure: Double = 0
    var recentColours: [Color] = []
    /// Incremented by the View menu to ask the canvas to fit.
    var fitRequest = 0
    /// Normalized point and request id from the navigator.
    var navigateX: Double = 0.5
    var navigateY: Double = 0.5
    var navigateRequest = 0
    /// Visible document region, normalized for the navigator rectangle.
    var viewport = CGRect(x: 0, y: 0, width: 1, height: 1)
}
