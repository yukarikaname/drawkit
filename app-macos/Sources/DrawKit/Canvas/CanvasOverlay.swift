// The transparent NSView above the Metal layer: figure, handles, cursor and paper border.

import SwiftUI
import AppKit

/// The figure, handles, cursor and border are NSView drawing; keeping them above the Metal layer
/// avoids shaders for every transient pixel. It is not interactive.
final class CanvasOverlayView: NSView {
    weak var canvas: CanvasView?

    override var isOpaque: Bool { false }

    override init(frame frameRect: NSRect) {
        super.init(frame: frameRect)
        wantsLayer = true
        layer?.backgroundColor = NSColor.clear.cgColor
        layer?.isOpaque = false
    }

    required init?(coder: NSCoder) {
        super.init(coder: coder)
        wantsLayer = true
        layer?.backgroundColor = NSColor.clear.cgColor
        layer?.isOpaque = false
    }

    override func hitTest(_ point: NSPoint) -> NSView? {
        nil
    }

    override func draw(_ dirtyRect: NSRect) {
        canvas?.drawOverlay(dirtyRect)
    }
}
