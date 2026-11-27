// The transparent NSView above the Metal layer: figure, handles, cursor and paper border.
//
// The NSView is deliberately unmirrored. SwiftUI may lay the shell out right-to-left, but the
// paper has a fixed orientation and every overlay coordinate comes from the pen or the paper
// frame, so the view keeps drawing in plain view coordinates.

import AppKit
import SwiftUI

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
