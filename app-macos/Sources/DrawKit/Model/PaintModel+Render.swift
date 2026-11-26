// Rendering the stroke tape at another output size, and the PNG panel that chooses it.
//
// The live canvas keeps painting at document resolution. An export that changes the size
// re-renders the recorded strokes at the target, so a thickness policy can keep line width in
// output pixels instead of scaling the finished pixels.

import AppKit
import UniformTypeIdentifiers

/// Size and thickness choices for the PNG export.
final class RenderOptionsView: NSView {
    private let percentages = [25, 50, 100, 200]
    private let policies: [RenderThickness] = [.recorded, .canvas, .screen]
    private let scalePopup = NSPopUpButton()
    private let policyPopup = NSPopUpButton()

    var percent: Int { percentages[max(0, scalePopup.indexOfSelectedItem)] }
    var thickness: RenderThickness {
        policies[max(0, min(policyPopup.indexOfSelectedItem, policies.count - 1))]
    }

    init() {
        super.init(frame: NSRect(x: 0, y: 0, width: 280, height: 56))
        scalePopup.addItems(withTitles: percentages.map { "\($0)%" })
        scalePopup.selectItem(at: 2)
        policyPopup.addItems(withTitles: policies.map { String(localized: $0.title) })
        let stack = NSStackView(views: [row("Size", scalePopup), row("Thickness", policyPopup)])
        stack.orientation = .vertical
        stack.alignment = .leading
        stack.spacing = 6
        stack.frame = bounds
        stack.autoresizingMask = [.width, .height]
        addSubview(stack)
    }

    required init?(coder: NSCoder) {
        nil
    }

    private func row(_ title: String, _ control: NSView) -> NSView {
        let label = NSTextField(labelWithString: title)
        label.font = .systemFont(ofSize: NSFont.smallSystemFontSize)
        label.setContentHuggingPriority(.defaultHigh, for: .horizontal)
        let row = NSStackView(views: [label, control])
        row.orientation = .horizontal
        row.spacing = 8
        return row
    }
}

extension PaintModel {
    /// Save what the canvas shows at the chosen output. Strokes are re-rendered, so `Fixed on
    /// screen` keeps their width; a document the renderer cannot replay falls back to resampling
    /// the finished composite.
    func exportPNG() {
        flushDeferred()
        if previewMs != nil {
            previewMs = nil
            refresh()
        }
        let options = RenderOptionsView()
        guard let url = askForPNGURL(accessory: options) else { return }
        let spec = OutputSpec.scaled(width: width, height: height,
                                     percent: options.percent, thickness: options.thickness)
        if options.percent != 100, let pixels = renderTape(at: spec),
           let image = makeImage(pixels: pixels, width: spec.width, height: spec.height) {
            if writePNG(image, to: url) {
                flash(String(localized: "Exported"))
            } else {
                NSSound.beep()
                flash(String(localized: "Could not export"), error: true)
            }
            return
        }
        guard let composite = compositeImage() else { return }
        let image = options.percent == 100 ? composite : resample(composite, to: spec)
        let written = writePNG(image, to: url)
        if !written {
            NSSound.beep()
            flash(String(localized: "Could not export"), error: true)
        } else if options.percent == 100 {
            flash(String(localized: "Exported"))
        } else {
            flash(String(localized: "Exported a resampled image; this document could not be re-rendered"))
        }
    }

    /// Replay the whole tape at `spec` as premultiplied RGBA8. Nil when the compositor cannot
    /// reproduce the document at that size (an extended-range output, for now).
    func renderTape(at spec: OutputSpec) -> [UInt8]? {
        guard let core = handle,
              let replay = TapeReplay(handle: core, width: width, height: height, spec: spec),
              !replay.isHdr else { return nil }
        var pixels = [UInt8](repeating: 0, count: spec.width * spec.height * 4)
        let drawn = pixels.withUnsafeMutableBytes { buffer in
            guard let base = buffer.bindMemory(to: UInt8.self).baseAddress else { return false }
            return replay.frame(at: UInt64.max, into: base)
        }
        return drawn ? pixels : nil
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
        guard let context = CGContext(
            data: nil, width: spec.width, height: spec.height, bitsPerComponent: 8,
            bytesPerRow: 0, space: colorSpace.cgColorSpace,
            bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue) else { return image }
        context.interpolationQuality = .high
        context.draw(image, in: CGRect(x: 0, y: 0, width: spec.width, height: spec.height))
        return context.makeImage() ?? image
    }

    /// Write the paper and the painting as one PNG. The document is transparent where nothing is
    /// drawn, but the painter sees paper, so the file gets the paper rather than a checkerboard.
    @discardableResult
    private func writePNG(_ image: CGImage, to url: URL) -> Bool {
        guard let context = CGContext(
            data: nil, width: image.width, height: image.height, bitsPerComponent: 8,
            bytesPerRow: 0, space: colorSpace.cgColorSpace,
            bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue) else { return false }
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
