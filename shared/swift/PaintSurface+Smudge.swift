// Smudge segments: each one moves paint into the resident tiles, and the journal groups them
// into one undo step.

import Foundation

@_silgen_name("dk_canvas_surface_set_stroke_smudge")
func dk_canvas_surface_set_stroke_smudge(
    _ handle: UnsafeMutableRawPointer?, _ dx: Float, _ dy: Float)
@_silgen_name("dk_canvas_surface_smudge_begin")
func dk_canvas_surface_smudge_begin(_ handle: UnsafeMutableRawPointer?, _ layer: UInt32)
@_silgen_name("dk_canvas_surface_smudge_end")
func dk_canvas_surface_smudge_end(_ handle: UnsafeMutableRawPointer?) -> Int32

extension PaintSurface {
    /// Make the next commit sample the layer at this offset, for one smudge segment.
    func setStrokeSmudge(dx: Float, dy: Float) {
        guard let handle else { return }
        dk_canvas_surface_set_stroke_smudge(handle, dx, dy)
    }

    func beginSmudgeJournal(layer: Int) {
        guard let handle else { return }
        dk_canvas_surface_smudge_begin(handle, UInt32(max(0, layer)))
    }

    /// Close the smudge's segment journal. Returns the number of committed rectangles.
    func endSmudgeJournal() -> Int {
        guard let handle else { return 0 }
        return Int(dk_canvas_surface_smudge_end(handle))
    }

}
