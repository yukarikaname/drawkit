// The iPad's document. One core editor, one RGBA buffer, one CGImage, and the tool state the
// chrome edits. The canvas view draws the image; the core composites only dirty rectangles into
// the buffer, so a stroke never re-reads the whole canvas.

import SwiftUI
import UIKit
import Observation
import CoreGraphics
import QuartzCore
import PencilKit

struct SurfaceDirty {
    var x: Int
    var y: Int
    var width: Int
    var height: Int
}

@MainActor
@Observable
final class Studio {
    let width = 1024
    let height = 1024
    var tool: IOSTool = .brush
    var color: Color = Color(red: 0.10, green: 0.09, blue: 0.08)
    var brushSize: Double = 18
    var opacity: Double = 1
    var brushPreset: BrushPreset = .softRound {
        didSet { dk_set_brush(handle, Int32(brushPreset.rawValue)) }
    }
    var tolerance: Double = 0.12
    var fillClose: Double = 1
    var fillExpand: Double = 1
    var fillFeather: Double = 0
    var stabilization: Double = 0 {
        didSet { dk_set_stabilization(handle, Float(stabilization * 0.9)) }
    }
    /// Replace a finished stroke with the shape the tiny on-device model recognises.
    var autoShape: Bool = false {
        didSet { dk_set_auto_shape(handle, autoShape ? 1 : 0) }
    }
    /// A held stroke snaps even when Auto Shape is off.
    var quickShape: Bool = true {
        didSet { dk_set_quick_shape(handle, quickShape ? 1 : 0) }
    }
    /// 0 off, 1 horizontal, 2 vertical, 3 both.
    var mirror: Int = 0 {
        didSet {
            dk_set_mirror(handle, Int32(mirror))
            revision += 1
        }
    }
    /// The working space follows the core's default for now; the document format carries it.
    var colorSpace: PaintColorSpace = .sRGB
    private(set) var hdr = false
    /// Adobe RGB has no eight-bit drawable, so it presents through the linear path like HDR.
    var needsLinear: Bool { hdr || colorSpace == .adobeRgb }
    /// What the GPU surface still needs from the last composite.
    private(set) var surfaceDirty: SurfaceDirty?
    private(set) var image: CGImage?
    var layers: [IOSLayer] = []
    var activeLayer = 0
    var canUndo = false
    var canRedo = false
    var isBlank = true
    var revision = 0
    /// Rectangular selection in document pixels; transient.
    var selection: CGRect?
    var recent: [Color] = []
    var showLayers = false
    var showColours = false
    var showPencilKit = false
    var showImport = false
    var showSettings = false
    var showExporter = false
    var exportName = "DrawKit.kra"
    var exportType = "kra"
    var exportPayload: Data?
    nonisolated(unsafe) let handle: UnsafeMutableRawPointer?
    nonisolated(unsafe) private let buffer: UnsafeMutablePointer<UInt8>
    private var scratch: [UInt8] = []
    private var linear: LinearBuffer?
    var lastTime: UInt64 = 0
    @ObservationIgnored var fillTask: Task<Void, Never>?
    private var strokeErased = false
    private let bytesPerRow: Int

    init() {
        let size = width * height * 4
        bytesPerRow = width * 4
        buffer = UnsafeMutablePointer<UInt8>.allocate(capacity: size)
        buffer.initialize(repeating: 0, count: size)
        handle = dk_editor_new(UInt32(width), UInt32(height))
        dk_set_brush(handle, Int32(brushPreset.rawValue))
        dk_set_quick_shape(handle, 1)
        refreshAll()
        refreshLayers()
    }

    deinit {
        dk_editor_free(handle)
        buffer.deinitialize(count: width * height * 4)
        buffer.deallocate()
    }
    /// Turn extended range on or off for this document. Values already painted stay as they are;
    /// the new range only lets the brush go above paper white from here.
    func setHdr(_ value: Bool) {
        guard value != hdr else { return }
        hdr = value
        dk_set_dynamic_range(handle, value ? 1 : 0)
        linear = needsLinear ? LinearBuffer(count: width * height * 4) : nil
        refreshAll()
        refreshLayers()
        revision += 1
    }

    func withSurfacePixels(hdr wantLinear: Bool,
                           _ body: (UnsafeMutableRawPointer, Int, Int, Bool) -> Void) {
        if wantLinear, needsLinear, let linear {
            body(UnsafeMutableRawPointer(linear.pointer), width, height, true)
        } else {
            body(UnsafeMutableRawPointer(buffer), width, height, false)
        }
    }

    func takeSurfaceDirty() -> SurfaceDirty? {
        let value = surfaceDirty
        surfaceDirty = nil
        return value
    }

    func markSurfaceFull() {
        surfaceDirty = SurfaceDirty(x: 0, y: 0, width: width, height: height)
    }

    private func markSurfaceDirty(x: Int, y: Int, width: Int, height: Int) {
        guard let current = surfaceDirty else {
            surfaceDirty = SurfaceDirty(x: x, y: y, width: width, height: height)
            return
        }
        let left = min(current.x, x)
        let top = min(current.y, y)
        let right = max(current.x + current.width, x + width)
        let bottom = max(current.y + current.height, y + height)
        surfaceDirty = SurfaceDirty(x: left, y: top, width: right - left, height: bottom - top)
    }

    // MARK: Drawing

    func beginStroke() {
        stampTime()
        dk_stroke_begin(handle, lastTime)
        strokeErased = tool == .eraser
    }

    func segment(from: CGPoint, to: CGPoint, pressure: CGFloat,
                 tilt: [Float], azimuth: Float, roll: Float) {
        stampTime()
        let radius = Float(brushSize / 2)
        let amount = Float(min(max(pressure, 0.05), 1))
        if strokeErased {
            dk_erase_ex(handle, Float(from.x), Float(from.y), Float(to.x), Float(to.y),
                        radius, amount, tilt[0], tilt[1], azimuth, roll, lastTime)
        } else {
            let components = colorComponents()
            dk_stroke_ex(handle, Float(from.x), Float(from.y), Float(to.x), Float(to.y),
                         radius, amount, tilt[0], tilt[1], azimuth, roll,
                         components.0, components.1, components.2,
                         Float(opacity), lastTime)
        }
        _ = refreshDirty()
    }

    func endStroke(quickShape held: Bool = false) {
        dk_stroke_end(handle)
        if autoShape || (held && quickShape), dk_auto_shape(handle) != 0 {
            refreshAll()
            refreshLayers()
        } else {
            _ = refreshDirty()
        }
        rememberColour()
        finishChange()
    }

    /// End a stroke that a gesture interrupted. No history was created if the pen never moved.
    func cancelStroke() {
        dk_stroke_end(handle)
        refreshLayers()
    }

    @discardableResult
    func pick(at point: CGPoint) -> Bool {
        var rgba = [Float](repeating: 0, count: 4)
        let found = rgba.withUnsafeMutableBufferPointer {
            dk_pick(handle, UInt32(max(0, point.x)), UInt32(max(0, point.y)), $0.baseAddress)
        }
        guard found != 0 else { return false }
        color = Color(.sRGB, red: Double(rgba[0]), green: Double(rgba[1]),
                      blue: Double(rgba[2]), opacity: Double(rgba[3]))
        rememberColour()
        return true
    }

    func setSelection(_ rect: CGRect?) {
        if let rect {
            dk_set_selection(handle, UInt32(max(0, rect.minX)), UInt32(max(0, rect.minY)),
                             UInt32(max(0, rect.width)), UInt32(max(0, rect.height)))
            selection = rect
        } else {
            dk_clear_selection(handle)
            selection = nil
        }
        revision += 1
    }

    @discardableResult
    func extractSelection() -> Bool {
        guard dk_extract_selection(handle) != 0 else { return false }
        refreshAll()
        refreshLayers()
        finishChange()
        return true
    }

    func setStabilization(_ value: Double) {
        stabilization = value
    }

    func undo() {
        guard dk_undo(handle) != 0 else { return }
        refreshAll()
        refreshLayers()
        finishChange()
    }

    func redo() {
        guard dk_redo(handle) != 0 else { return }
        refreshAll()
        refreshLayers()
        finishChange()
    }

    func clearLayer() {
        dk_clear(handle, 1, 1, 1, 0)
        refreshAll()
        finishChange()
    }

    // MARK: PencilKit

    /// Bake a PencilKit drawing into a new layer. The drawing is rasterised to the document size
    /// once; after that it is ordinary paint and every brush and undo step applies to it.
    @discardableResult
    func addPencilKitDrawing(_ drawing: PKDrawing) -> Bool {
        let bounds = CGRect(x: 0, y: 0, width: width, height: height)
        let source = drawing.bounds.isEmpty ? bounds : drawing.bounds
        let image = drawing.image(from: source, scale: 1)
        guard let cgImage = image.cgImage else { return false }
        var pixels = [UInt8](repeating: 0, count: width * height * 4)
        let drawn: Bool = pixels.withUnsafeMutableBytes { raw in
            guard let context = CGContext(
                data: raw.baseAddress, width: width, height: height,
                bitsPerComponent: 8, bytesPerRow: width * 4,
                space: CGColorSpaceCreateDeviceRGB(),
                bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue
            ) else { return false }
            context.setFillColor(UIColor.clear.cgColor)
            context.fill(bounds)
            // Core surfaces are top-down; a bitmap context is bottom-up, so flip before drawing.
            context.translateBy(x: 0, y: CGFloat(height))
            context.scaleBy(x: 1, y: -1)
            let scale = min(CGFloat(width) / source.width, CGFloat(height) / source.height)
            let offsetX = (CGFloat(width) - source.width * scale) / 2
            let offsetY = (CGFloat(height) - source.height * scale) / 2
            context.draw(cgImage, in: CGRect(
                x: offsetX, y: offsetY,
                width: source.width * scale, height: source.height * scale
            ))
            return true
        }
        guard drawn else { return false }
        let added = pixels.withUnsafeBytes { raw in
            dk_layer_add_image(handle, UInt32(width), UInt32(height),
                               raw.bindMemory(to: UInt8.self).baseAddress)
        }
        guard added != 0 else { return false }
        refreshAll()
        refreshLayers()
        finishChange()
        return true
    }

    /// Replace the painting with a Krita or Photoshop document.
    @discardableResult
    func importDocument(_ url: URL) -> Bool {
        let scoped = url.startAccessingSecurityScopedResource()
        defer {
            if scoped { url.stopAccessingSecurityScopedResource() }
        }
        let imported = url.path.withCString { path -> Int32 in
            switch url.pathExtension.lowercased() {
            case "kra": dk_import_kra(handle, path)
            case "psd": dk_import_psd(handle, path)
            default: 0
            }
        }
        guard imported != 0 else {
            haptic(.error)
            return false
        }
        refreshAll()
        refreshLayers()
        finishChange()
        haptic(.success)
        return true
    }

    /// Write the current document to a temporary file and return its bytes for `fileExporter`.
    @discardableResult
    func prepareExport(_ format: String) -> Bool {
        let url = FileManager.default.temporaryDirectory
            .appendingPathComponent("DrawKit.\(format)")
        let saved = url.path.withCString { path -> Int32 in
            switch format {
            case "psd": dk_export_psd(handle, path)
            case "dkp": dk_save_tape(handle, path)
            case "exr": dk_export_exr(handle, path)
            case "dkp": dk_save_tape(handle, path)
            default: dk_export_kra(handle, path)
            }
        }
        guard saved != 0, let data = try? Data(contentsOf: url) else {
            haptic(.error)
            return false
        }
        try? FileManager.default.removeItem(at: url)
        exportPayload = data
        exportType = format
        exportName = "DrawKit.\(format)"
        showExporter = true
        return true
    }

    // MARK: Colours

    /// Sample the painting's dominant colours into the recent row.
    func sampleArtwork() {
        var values = [Float](repeating: 0, count: 24)
        let written = values.withUnsafeMutableBufferPointer {
            dk_palette(handle, 6, $0.baseAddress)
        }
        guard written > 0 else { return }
        for index in 0..<Int(written) {
            let colour = Color(.sRGB,
                               red: Double(values[index * 4]),
                               green: Double(values[index * 4 + 1]),
                               blue: Double(values[index * 4 + 2]),
                               opacity: Double(values[index * 4 + 3]))
            recent.removeAll { $0 == colour }
            recent.insert(colour, at: 0)
        }
        recent = Array(recent.prefix(10))
        if let first = recent.first { color = first }
    }

    func rememberColour() {
        if !recent.contains(where: { $0 == color }) {
            recent.insert(color, at: 0)
            recent = Array(recent.prefix(10))
        }
    }

    var hex: String {
        let ui = UIColor(color)
        var red: CGFloat = 0
        var green: CGFloat = 0
        var blue: CGFloat = 0
        var alpha: CGFloat = 1
        ui.getRed(&red, green: &green, blue: &blue, alpha: &alpha)
        return String(format: "#%02X%02X%02X",
                      Int(red * 255), Int(green * 255), Int(blue * 255))
    }

    // MARK: Pixels

    func refreshAll() {
        _ = dk_composite(handle, buffer)
        image = makeImage()
        revision += 1
    }

    func refreshDirty() -> Bool {
        var rect = [UInt32](repeating: 0, count: 4)
        let dirty = rect.withUnsafeMutableBufferPointer {
            dk_take_dirty(handle, $0.baseAddress)
        }
        guard dirty != 0 else { return false }
        let x = Int(rect[0])
        let y = Int(rect[1])
        let rectWidth = Int(rect[2])
        let rectHeight = Int(rect[3])
        let count = rectWidth * rectHeight * 4
        guard count > 0 else { return false }
        if scratch.count < count {
            scratch = [UInt8](repeating: 0, count: count)
        }
        _ = scratch.withUnsafeMutableBufferPointer {
            dk_composite_rect(handle, UInt32(x), UInt32(y), UInt32(rectWidth), UInt32(rectHeight),
                              $0.baseAddress)
        }
        scratch.withUnsafeBufferPointer { source in
            guard let base = source.baseAddress else { return }
            for row in 0..<rectHeight {
                let destination = buffer.advanced(by: ((y + row) * self.width + x) * 4)
                destination.update(from: base.advanced(by: row * rectWidth * 4),
                                   count: rectWidth * 4)
            }
        }
        if let linear {
            compositeLinear(x: x, y: y, width: rectWidth, height: rectHeight)
        }
        markSurfaceDirty(x: x, y: y, width: rectWidth, height: rectHeight)
        image = makeImage()
        isBlank = false
        revision += 1
        return true
    }

    private func compositeLinear(x: Int, y: Int, width: Int, height: Int) {
        guard let linear else { return }
        var tile = [Float](repeating: 0, count: width * height * 4)
        let drawn = tile.withUnsafeMutableBufferPointer {
            dk_composite_linear_rect(handle, UInt32(x), UInt32(y),
                                     UInt32(width), UInt32(height), $0.baseAddress)
        }
        guard drawn != 0 else { return }
        tile.withUnsafeBufferPointer { source in
            guard let base = source.baseAddress else { return }
            for row in 0..<height {
                let destination = linear.pointer + ((y + row) * self.width + x) * 4
                destination.update(from: base + row * width * 4, count: width * 4)
            }
        }
    }

    private func makeImage() -> CGImage? {
        let provider = CGDataProvider(
            dataInfo: nil,
            data: UnsafeRawPointer(buffer),
            size: width * height * 4,
            releaseData: { _, _, _ in }
        )
        guard let provider else { return nil }
        return CGImage(
            width: width, height: height, bitsPerComponent: 8, bitsPerPixel: 32,
            bytesPerRow: bytesPerRow, space: CGColorSpaceCreateDeviceRGB(),
            bitmapInfo: CGBitmapInfo(rawValue: CGImageAlphaInfo.premultipliedLast.rawValue),
            provider: provider, decode: nil, shouldInterpolate: false, intent: .defaultIntent
        )
    }

    func finishChange() {
        canUndo = dk_can_undo(handle) != 0
        canRedo = dk_can_redo(handle) != 0
        isBlank = dk_is_clear(handle) != 0
        revision += 1
    }

    func stampTime() {
        let now = UInt64(CACurrentMediaTime() * 1000)
        lastTime = max(lastTime + 1, now)
    }

    private func haptic(_ type: UINotificationFeedbackGenerator.FeedbackType) {
        MainActor.assumeIsolated {
            UINotificationFeedbackGenerator().notificationOccurred(type)
        }
    }

    func colorComponents() -> (Float, Float, Float) {
        let ui = UIColor(color)
        var red: CGFloat = 0
        var green: CGFloat = 0
        var blue: CGFloat = 0
        var alpha: CGFloat = 1
        ui.getRed(&red, green: &green, blue: &blue, alpha: &alpha)
        return (Float(red), Float(green), Float(blue))
    }
}
