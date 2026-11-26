// The NSView bridge to the AppKit canvas: one struct, separate so ContentView stays a layout file.

import SwiftUI
import AppKit

struct CanvasRepresentable: NSViewRepresentable {
    let model: PaintModel
    let figure: FigureModel
    let tool: Tool
    let size: Double
    let eraserOpacity: Double
    let smudgeStrength: Double
    let zoom: Double
    let fitRequest: Int
    let rotationRequest: Int
    let rotation: Double
    let navigateX: Double
    let navigateY: Double
    let navigateRequest: Int
    let panX: Double
    let panY: Double
    let shortcuts: ShortcutStore
    let revision: UInt64
    let tolerance: Double
    let fillClose: Double
    let fillExpand: Double
    let fillFeather: Double
    let mirrorEnabled: Bool
    let mirrorAngle: Double
    let mirrorBoth: Bool
    let colorSpace: PaintColorSpace
    let extended: Bool
    let exposure: Double
    let pressureCurve: Double
    let pressureLow: Double
    let pressureHigh: Double
    let ink: Ink
    let tabletPrimary: TabletButtonAction
    let tabletSecondary: TabletButtonAction
    let onTabletDetected: () -> Void
    let recentColours: [Color]
    let currentBrush: BrushKind
    let onBrush: (BrushKind) -> Void
    let onColor: (Color) -> Void
    let onTool: (Tool) -> Void
    let onSize: (Double) -> Void
    let onPick: (Color, Double) -> Void
    let onZoom: (Double) -> Void
    let onPan: (CGPoint) -> Void
    let onViewport: (CGRect) -> Void
    let onRotation: (CGFloat) -> Void

    func makeNSView(context: Context) -> CanvasView {
        let view = CanvasView()
        view.model = model
        view.shortcuts = shortcuts
        view.onTool = onTool
        view.onSize = { onSize(Double($0)) }
        view.onPick = onPick
        view.onZoom = { onZoom(Double($0)) }
        view.onPan = onPan
        view.onViewport = onViewport
        view.colorSpace = colorSpace
        view.tabletPrimary = tabletPrimary
        view.tabletSecondary = tabletSecondary
        view.onTabletDetected = onTabletDetected
        view.recentColours = recentColours
        view.currentBrush = currentBrush
        view.onBrush = onBrush
        view.onColor = onColor
        view.onRotation = { radians in
            onRotation(radians)
        }
        return view
    }

    func updateNSView(_ view: CanvasView, context: Context) {
        // A SwiftUI update is not a place to flush Core Animation: the hosting view is mid-layout
        // and a synchronous commit makes it lay out again inside itself.
        view.inUpdatePass = true
        defer { view.inUpdatePass = false }
        view.model = model
        view.refreshGPUAvailability()
        view.shortcuts = shortcuts
        view.tool = tool
        view.ink = ink
        view.radius = CGFloat(size)
        view.eraserOpacity = eraserOpacity
        view.smudgeStrength = smudgeStrength
        view.tolerance = Float(tolerance)
        view.fillClose = Int(fillClose)
        view.fillExpand = Int(fillExpand)
        view.fillFeather = Int(fillFeather)
        view.mirrorEnabled = mirrorEnabled
        view.mirrorAngle = mirrorAngle
        view.mirrorBoth = mirrorBoth
        view.onionImage = model.onionImage
        view.setColorSpace(colorSpace)
        view.setExtended(extended)
        view.setExposure(exposure)
        view.pressureCurve = pressureCurve
        view.pressureLow = pressureLow
        view.pressureHigh = pressureHigh
        view.tabletPrimary = tabletPrimary
        view.tabletSecondary = tabletSecondary
        view.onTabletDetected = onTabletDetected
        view.recentColours = recentColours
        view.currentBrush = currentBrush
        view.onBrush = onBrush
        view.onColor = onColor
        view.setExternalZoom(CGFloat(zoom))
        view.setExternalRotation(CGFloat(rotation))
        view.setExternalPan(CGPoint(x: panX, y: panY))
        if view.fitRequest != fitRequest {
            view.fitRequest = fitRequest
            view.fit()
        }
        if view.rotationRequest != rotationRequest {
            view.rotationRequest = rotationRequest
            view.resetRotation()
        }
        if view.navigateRequest != navigateRequest {
            view.navigateRequest = navigateRequest
            view.center(onNormalized: CGPoint(x: navigateX, y: navigateY))
        }
        // A live resize already has a present queued from the layout pass; this update belongs to
        // the same turn, so it joins that frame instead of composing a second one.
        if view.window?.inLiveResize == true {
            view.schedulePresent()
        } else {
            view.presentSurface()
            // Writing the viewport back into SwiftUI state re-evaluates the editor body. During a
            // drag that happens on every step, so the navigator catches up when the drag ends.
            view.reportViewport()
        }
    }
}
