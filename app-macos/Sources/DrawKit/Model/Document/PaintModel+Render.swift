// Rendering the stroke tape at another output size, and the PNG panel that chooses it.
//
// The live canvas keeps painting at document resolution. An export that changes the size
// re-renders the recorded strokes at the target, so a thickness policy can keep line width in
// output pixels instead of scaling the finished pixels.

import AppKit
import UniformTypeIdentifiers

/// Size and thickness choices for the PNG export. The print target is the same re-render, sized
/// from paper and DPI instead of a percentage, so a 300 dpi A4 does not start from a resampled
/// screen image.
final class RenderOptionsView: NSView {
    private let percentages = [25, 50, 100, 200, 400]
    private let papers: [(name: String, width: CGFloat, height: CGFloat)] = [
        ("A4", 8.27, 11.69), ("A3", 11.69, 16.54), ("Letter", 8.5, 11), ("Tabloid", 11, 17),
    ]
    private let dpis = [150, 300, 600]
    private let policies: [RenderThickness] = [.recorded, .canvas, .screen]
    private let targetPopup = NSPopUpButton()
    private let scalePopup = NSPopUpButton()
    private let paperPopup = NSPopUpButton()
    private let dpiPopup = NSPopUpButton()
    private let policyPopup = NSPopUpButton()
    private let sizeLabel = NSTextField(labelWithString: "")
    /// The document the export starts from, for the print-fit calculation.
    var documentWidth = 0
    var documentHeight = 0

    var percent: Int {
        if targetPopup.indexOfSelectedItem == 0 {
            return percentages[max(0, scalePopup.indexOfSelectedItem)]
        }
        return printPercent
    }

    var thickness: RenderThickness {
        policies[max(0, min(policyPopup.indexOfSelectedItem, policies.count - 1))]
    }

    private var printPercent: Int {
        guard documentWidth > 0, documentHeight > 0 else { return 100 }
        let paper = papers[max(0, paperPopup.indexOfSelectedItem)]
        let dpi = CGFloat(dpis[max(0, dpiPopup.indexOfSelectedItem)])
        let factor = min(
            paper.width * dpi / CGFloat(documentWidth),
            paper.height * dpi / CGFloat(documentHeight))
        return max(1, Int((factor * 100).rounded()))
    }

    init() {
        super.init(frame: NSRect(x: 0, y: 0, width: 330, height: 132))
        targetPopup.addItems(withTitles: ["Scale", "Print size"])
        scalePopup.addItems(withTitles: percentages.map { "\($0)%" })
        scalePopup.selectItem(at: 2)
        for paper in papers { paperPopup.addItem(withTitle: paper.name) }
        dpiPopup.addItems(withTitles: dpis.map { "\($0) dpi" })
        dpiPopup.selectItem(at: 1)
        policyPopup.addItems(withTitles: policies.map { String(localized: $0.title) })

        for control in [targetPopup, scalePopup, paperPopup, dpiPopup, policyPopup] {
            control.target = self
            control.action = #selector(changed)
        }
        sizeLabel.font = .monospacedDigitSystemFont(
            ofSize: NSFont.smallSystemFontSize,
            weight: .regular)
        sizeLabel.textColor = .secondaryLabelColor

        let stack = NSStackView(views: [
            row("Target", targetPopup),
            row("Scale", scalePopup),
            row("Paper", paperPopup),
            row("Resolution", dpiPopup),
            row("Thickness", policyPopup),
            sizeLabel,
        ])
        stack.orientation = .vertical
        stack.alignment = .leading
        stack.spacing = 5
        stack.frame = bounds
        stack.autoresizingMask = [.width, .height]
        addSubview(stack)
        update()
    }

    required init?(coder: NSCoder) {
        nil
    }

    @objc private func changed() {
        update()
    }

    private func update() {
        let printing = targetPopup.indexOfSelectedItem == 1
        scalePopup.isEnabled = !printing
        paperPopup.isEnabled = printing
        dpiPopup.isEnabled = printing
        guard documentWidth > 0, documentHeight > 0 else { return }
        let factor = CGFloat(percent) / 100
        let width = Int((CGFloat(documentWidth) * factor).rounded())
        let height = Int((CGFloat(documentHeight) * factor).rounded())
        sizeLabel.stringValue = "\(width) × \(height) px"
    }

    private func row(_ title: String, _ control: NSView) -> NSView {
        let label = NSTextField(labelWithString: title)
        label.font = .systemFont(ofSize: NSFont.smallSystemFontSize)
        label.setContentHuggingPriority(.defaultHigh, for: .horizontal)
        let row = NSStackView(views: [label, control])
        row.orientation = .horizontal
        row.alignment = .firstBaseline
        row.spacing = 8
        return row
    }
}

extension PaintModel {
    /// Save what the canvas shows at the chosen output. Strokes are re-rendered, so `Fixed on
    /// screen` keeps their width; a document the renderer cannot replay falls back to resampling
    /// the finished composite.
    func exportPNG() {
        if previewMs != nil {
            previewMs = nil
            refresh()
        }
        let options = RenderOptionsView()
        options.documentWidth = width
        options.documentHeight = height
        guard let url = askForPNGURL(accessory: options) else { return }
        let spec = OutputSpec.scaled(
            width: width, height: height,
            percent: options.percent, thickness: options.thickness)
        if options.percent != 100, let pixels = renderTape(at: spec),
            let image = makeImage(pixels: pixels, width: spec.width, height: spec.height)
        {
            if writePNG(image, to: url) {
                flash(String(localized: "Exported"))
            } else {
                NSSound.beep()
                flash(String(localized: "Could not export"), error: true)
            }
            return
        }
        // The resident stack already holds the composed frame; only materialise when it cannot
        // supply the pixels.
        guard let composite = gpuCompositeImage(width: width, height: height) ?? cpuCompositeImage()
        else { return }
        let image = options.percent == 100 ? composite : resample(composite, to: spec)
        let written = writePNG(image, to: url)
        if !written {
            NSSound.beep()
            flash(String(localized: "Could not export"), error: true)
        } else if options.percent == 100 {
            flash(String(localized: "Exported"))
        } else {
            flash(
                String(
                    localized: "Exported a resampled image; this document could not be re-rendered")
            )
        }
    }

    /// The composed frame from the resident stack, resampled on the GPU. Nil when the surface
    /// cannot supply it: no surface, an extended-range document, or a stack the shell has not
    /// built yet.
    func gpuCompositeImage(width outWidth: Int, height outHeight: Int) -> CGImage? {
        guard let pixels = exportPixelsFromGPU(width: outWidth, height: outHeight) else {
            return nil
        }
        return makeImage(pixels: pixels, width: outWidth, height: outHeight)
    }

    /// The same frame composited by the core, after bringing its CPU pixels up to date.
    func cpuCompositeImage() -> CGImage? {
        flushDeferred()
        return compositeImage()
    }

    /// Composite, resample and read back on the GPU. Nil when the resident stack cannot supply
    /// the frame; the caller then materialises the core surface instead.
    func exportPixelsFromGPU(width outWidth: Int, height outHeight: Int) -> [UInt8]? {
        guard let surface = paintSurface, surface.supportsLayers,
            !gpuNeedsRebuild, surface.residentLayerCount == layers.count,
            !layers.indices.contains(where: { surface.layerMissing($0) }),
            outWidth > 0, outHeight > 0
        else { return nil }
        // Tiles the budget evicted live only in the core now; a hole in the frame is worse than
        // the upload it takes to fill it.
        guard restoreMissingTiles(surface) else { return nil }
        // The surface composites the same specs the view pushes, so push them even when no
        // present has run since the last layer edit.
        for index in 0..<layers.count {
            guard let state = gpuLayerState(index: index) else { return nil }
            surface.setLayer(
                index: index, opacity: state.opacity, blend: Int(state.blend),
                visible: state.visible, fill: state.fill)
            surface.setLayerParent(index: index, parent: state.parent)
            surface.setLayerClip(index: index, base: Int32(gpuLayerClip(index: index)))
            surface.setExportReference(index: index, reference: layers[index].reference)
        }
        gpuSpecsUpdated()
        return surface.exportPixels(width: outWidth, height: outHeight)
    }

    /// Upload the tiles the resident budget evicted, so an export cannot contain a hole.
    private func restoreMissingTiles(_ surface: PaintSurface) -> Bool {
        // Each pass tops the stack up; a few cover a stack the budget evicted wholesale. If the
        // budget keeps evicting, give up and let the caller materialise.
        for _ in 0..<64 {
            let missing = surface.missingRects(limit: 256)
            if missing.isEmpty { return true }
            for rect in missing {
                guard
                    uploadLayerRect(
                        surface, index: rect.layer, x: rect.x, y: rect.y,
                        width: rect.width, height: rect.height)
                else { return false }
            }
        }
        return surface.missingRects(limit: 1).isEmpty
    }

    /// Replay the whole tape at `spec` as premultiplied RGBA8. Nil when the compositor cannot
    /// reproduce the document at that size (an extended-range output, for now).
    func renderTape(at spec: OutputSpec) -> [UInt8]? {
        guard let core = handle,
            let replay = TapeReplay(handle: core, width: width, height: height, spec: spec),
            !replay.isHdr
        else { return nil }
        var pixels = [UInt8](repeating: 0, count: spec.width * spec.height * 4)
        let drawn = pixels.withUnsafeMutableBytes { buffer in
            guard let base = buffer.bindMemory(to: UInt8.self).baseAddress else { return false }
            return replay.frame(at: UInt64.max, into: base)
        }
        return drawn ? pixels : nil
    }

    /// The whole painting over paper as PNG, for a drag to Finder or another app.
    func documentPNGData() -> Data? {
        guard let composite = compositeImage() else { return nil }
        let bounds = CGRect(x: 0, y: 0, width: composite.width, height: composite.height)
        guard
            let context = CGContext(
                data: nil, width: composite.width, height: composite.height, bitsPerComponent: 8,
                bytesPerRow: 0, space: colorSpace.cgColorSpace,
                bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)
        else { return nil }
        context.setFillColor(NSColor.white.cgColor)
        context.fill(bounds)
        context.draw(composite, in: bounds)
        guard let flattened = context.makeImage() else { return nil }
        return NSBitmapImageRep(cgImage: flattened).representation(using: .png, properties: [:])
    }

    /// One layer as PNG (premultiplied, no paper), for a drag out of the palette.
    func layerPNGData(index: Int) -> Data? {
        guard index >= 0, index < layers.count else { return nil }
        materializeCanvas()
        var pixels = [UInt8](repeating: 0, count: width * height * 4)
        let read = pixels.withUnsafeMutableBytes {
            dk_layer_rect(
                handle, UInt32(index), 0, 0, UInt32(width), UInt32(height),
                $0.bindMemory(to: UInt8.self).baseAddress) != 0
        }
        guard read, let provider = CGDataProvider(data: Data(pixels) as CFData),
            let image = CGImage(
                width: width, height: height, bitsPerComponent: 8, bitsPerPixel: 32,
                bytesPerRow: width * 4, space: colorSpace.cgColorSpace,
                bitmapInfo: CGBitmapInfo(rawValue: CGImageAlphaInfo.premultipliedLast.rawValue),
                provider: provider, decode: nil, shouldInterpolate: false,
                intent: .defaultIntent)
        else { return nil }
        return NSBitmapImageRep(cgImage: image).representation(using: .png, properties: [:])
    }

    private func compositeImage() -> CGImage? {
        var rgba = [UInt8](repeating: 0, count: width * height * 4)
        let drawn = rgba.withUnsafeMutableBytes { buffer in
            dk_composite_export(handle, buffer.bindMemory(to: UInt8.self).baseAddress)
        }
        guard drawn != 0 else { return nil }
        return makeImage(pixels: rgba, width: width, height: height)
    }

    private func makeImage(pixels: [UInt8], width: Int, height: Int) -> CGImage? {
        guard let provider = CGDataProvider(data: Data(pixels) as CFData) else { return nil }
        return CGImage(
            width: width, height: height, bitsPerComponent: 8, bitsPerPixel: 32,
            bytesPerRow: width * 4, space: colorSpace.cgColorSpace,
            bitmapInfo: CGBitmapInfo(rawValue: CGImageAlphaInfo.premultipliedLast.rawValue),
            provider: provider, decode: nil, shouldInterpolate: true,
            intent: .defaultIntent)
    }

    private func resample(_ image: CGImage, to spec: OutputSpec) -> CGImage {
        guard
            let context = CGContext(
                data: nil, width: spec.width, height: spec.height, bitsPerComponent: 8,
                bytesPerRow: 0, space: colorSpace.cgColorSpace,
                bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)
        else { return image }
        context.interpolationQuality = .high
        context.draw(image, in: CGRect(x: 0, y: 0, width: spec.width, height: spec.height))
        return context.makeImage() ?? image
    }

    /// Write the composed canvas to an explicit path, for App Intents. The interactive export
    /// above owns the panel and the scale options; an automation supplies both ahead of time.
    @discardableResult
    func exportPNG(to url: URL) -> Bool {
        flushDeferred()
        guard let composite = gpuCompositeImage(width: width, height: height) ?? cpuCompositeImage()
        else { return false }
        return writePNG(composite, to: url)
    }

    /// Write the paper and the painting as one PNG. The document is transparent where nothing is
    /// drawn, but the painter sees paper, so the file gets the paper rather than a checkerboard.
    @discardableResult
    private func writePNG(_ image: CGImage, to url: URL) -> Bool {
        guard
            let context = CGContext(
                data: nil, width: image.width, height: image.height, bitsPerComponent: 8,
                bytesPerRow: 0, space: colorSpace.cgColorSpace,
                bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)
        else { return false }
        let bounds = CGRect(x: 0, y: 0, width: image.width, height: image.height)
        context.setFillColor(NSColor.white.cgColor)
        context.fill(bounds)
        context.draw(image, in: bounds)
        guard let flattened = context.makeImage(),
            let destination = CGImageDestinationCreateWithURL(
                url as CFURL, UTType.png.identifier as CFString, 1, nil)
        else { return false }
        CGImageDestinationAddImage(destination, flattened, nil)
        return CGImageDestinationFinalize(destination)
    }

    private func askForPNGURL(accessory: NSView?) -> URL? {
        let panel = NSSavePanel()
        panel.allowedContentTypes = [.png]
        panel.nameFieldStringValue = "drawkit.png"
        panel.accessoryView = accessory
        return panel.runModal() == .OK ? panel.url : nil
    }
}
