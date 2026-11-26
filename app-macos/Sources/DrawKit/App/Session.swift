// The window state that is not the document: which painting was open, the tool and colour, the
// zoom. Reopening the app where it was left is a desktop habit; the document itself still comes
// from the project file or the recovery copy, never from this JSON.

import AppKit
import Foundation
import SwiftUI

struct Session: Codable {
    var document: String?
    var tool: String
    var brush: Int
    var size: Double
    var eraserSize: Double?
    var eraserOpacity: Double?
    var smudgeStrength: Double?
    var stabilization: Double
    var tolerance: Double
    var fillClose: Double
    var fillExpand: Double
    var fillFeather: Double
    var fillSnap: Bool?
    var strokeDedupe: Bool?
    var prediction: Bool?
    var autoShape: Bool?
    var quickShape: Bool?
    var zoom: Double
    var rotation: Double?
    var panX: Double?
    var panY: Double?
    var recent: [[Double]]?
    var mirrorEnabled: Bool
    var mirrorAngle: Double
    var mirrorBoth: Bool
    var glow: Double
    var exposure: Double
    var colour: [Double]
    var activeLayer: Int
    var savedAt: Date

    static var url: URL {
        let base = FileManager.default.urls(for: .applicationSupportDirectory,
                                            in: .userDomainMask).first
            ?? FileManager.default.temporaryDirectory
        return base.appendingPathComponent("DrawKit/session.json")
    }

    @MainActor
    static func current(model: PaintModel, workspace: Workspace) -> Session {
        let ns = NSColor(workspace.color).usingColorSpace(.sRGB)
            ?? NSColor.black.usingColorSpace(.sRGB) ?? .black
        return Session(
            document: model.documentURL?.path,
            tool: workspace.tool.rawValue,
            brush: workspace.brush.rawValue,
            size: workspace.size,
            eraserSize: workspace.eraserSize,
            eraserOpacity: workspace.eraserOpacity,
            smudgeStrength: workspace.smudgeStrength,
            stabilization: workspace.stabilization,
            tolerance: workspace.tolerance,
            fillClose: workspace.fillClose,
            fillExpand: workspace.fillExpand,
            fillFeather: workspace.fillFeather,
            fillSnap: workspace.fillSnap,
            strokeDedupe: workspace.strokeDedupe,
            prediction: workspace.prediction,
            autoShape: workspace.autoShape,
            quickShape: workspace.quickShape,
            zoom: workspace.zoom,
            rotation: workspace.rotation,
            panX: workspace.panX,
            panY: workspace.panY,
            recent: workspace.recentColours.map { colour in
                guard let ns = NSColor(colour).usingColorSpace(.sRGB) else {
                    return [0, 0, 0, 1]
                }
                return [ns.redComponent, ns.greenComponent, ns.blueComponent, ns.alphaComponent]
            },
            mirrorEnabled: workspace.mirrorEnabled,
            mirrorAngle: workspace.mirrorAngle,
            mirrorBoth: workspace.mirrorBoth,
            glow: workspace.glow,
            exposure: workspace.exposure,
            colour: [ns.redComponent, ns.greenComponent, ns.blueComponent, ns.alphaComponent],
            activeLayer: model.activeLayer,
            savedAt: Date()
        )
    }

    @MainActor
    static func save(model: PaintModel, workspace: Workspace) {
        guard model.documentURL != nil else {
            // An untitled painting is the recovery copy's job; a path here would point at a file
            // that does not exist.
            try? FileManager.default.removeItem(at: url)
            return
        }
        let directory = url.deletingLastPathComponent()
        try? FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        if let data = try? JSONEncoder().encode(current(model: model, workspace: workspace)) {
            try? data.write(to: url, options: .atomic)
        }
    }

    static func load() -> Session? {
        guard let data = try? Data(contentsOf: url),
              let session = try? JSONDecoder().decode(Session.self, from: data),
              let document = session.document,
              FileManager.default.fileExists(atPath: document)
        else { return nil }
        return session
    }

    /// Put the saved window state back. The caller opens the document first, so values that the
    /// document owns (colour space, size) are already settled.
    @MainActor
    func apply(to model: PaintModel, workspace: Workspace) {
        if let restored = Tool(rawValue: tool) {
            workspace.tool = restored
        }
        if let restored = BrushKind(rawValue: brush) {
            workspace.brush = restored
            model.setBrush(restored)
        }
        workspace.size = size
        workspace.eraserSize = eraserSize ?? 24
        workspace.eraserOpacity = eraserOpacity ?? 1
        workspace.smudgeStrength = smudgeStrength ?? 0.6
        workspace.stabilization = stabilization
        workspace.tolerance = tolerance
        workspace.fillClose = fillClose
        workspace.fillExpand = fillExpand
        workspace.fillFeather = fillFeather
        workspace.fillSnap = fillSnap ?? true
        workspace.strokeDedupe = strokeDedupe ?? true
        workspace.prediction = prediction ?? true
        workspace.autoShape = autoShape ?? false
        workspace.quickShape = quickShape ?? true
        workspace.zoom = zoom
        workspace.rotation = rotation ?? 0
        workspace.panX = panX ?? 0
        workspace.panY = panY ?? 0
        if let recent, !recent.isEmpty {
            workspace.recentColours = recent.compactMap { colour in
                guard colour.count >= 4 else { return nil }
                return Color(.sRGB, red: colour[0], green: colour[1],
                             blue: colour[2], opacity: colour[3])
            }
        }
        workspace.mirrorEnabled = mirrorEnabled
        workspace.mirrorAngle = mirrorAngle
        workspace.mirrorBoth = mirrorBoth
        workspace.glow = glow
        workspace.exposure = exposure
        if colour.count >= 4 {
            workspace.color = Color(.sRGB, red: colour[0], green: colour[1],
                                   blue: colour[2], opacity: colour[3])
        }
        if activeLayer > 0 && activeLayer < model.layers.count {
            model.selectLayer(activeLayer)
        }
        model.setStabilization(stabilization / 10 * 0.9)
        model.setFillSnap(workspace.fillSnap)
        model.setStrokeDedupe(workspace.strokeDedupe)
        model.setPrediction(workspace.prediction)
        model.setAutoShape(workspace.autoShape)
        model.setQuickShape(workspace.quickShape)
        model.setMirror(enabled: mirrorEnabled, angle: mirrorAngle, both: mirrorBoth)
    }
}
