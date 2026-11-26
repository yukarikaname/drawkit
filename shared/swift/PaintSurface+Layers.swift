// Incremental resident-layer edits. A structural change moves hosts; it does not re-upload the
// layers it did not touch.

import Foundation

@_silgen_name("dk_canvas_surface_layer_count")
private func dk_canvas_surface_layer_count(_ handle: UnsafeMutableRawPointer?) -> UInt32

@_silgen_name("dk_canvas_surface_insert_layer")
private func dk_canvas_surface_insert_layer(
    _ handle: UnsafeMutableRawPointer?, _ index: UInt32,
    _ width: UInt32, _ height: UInt32) -> Int32

@_silgen_name("dk_canvas_surface_remove_layer")
private func dk_canvas_surface_remove_layer(
    _ handle: UnsafeMutableRawPointer?, _ index: UInt32) -> Int32

@_silgen_name("dk_canvas_surface_move_layer")
private func dk_canvas_surface_move_layer(
    _ handle: UnsafeMutableRawPointer?, _ from: UInt32, _ to: UInt32) -> Int32

extension PaintSurface {
    /// Layers the resident stack holds. Comparing this with the document avoids a rebuild after
    /// an incremental insert or remove.
    var residentLayerCount: Int {
        guard let handle else { return 0 }
        return Int(dk_canvas_surface_layer_count(handle))
    }

    func insertResidentLayer(index: Int, width: Int, height: Int) -> Bool {
        guard let handle, index >= 0, width > 0, height > 0 else { return false }
        return dk_canvas_surface_insert_layer(
            handle, UInt32(index), UInt32(width), UInt32(height)) != 0
    }

    func removeResidentLayer(index: Int) -> Bool {
        guard let handle, index >= 0 else { return false }
        return dk_canvas_surface_remove_layer(handle, UInt32(index)) != 0
    }

    func moveResidentLayer(from: Int, to: Int) -> Bool {
        guard let handle, from >= 0, to >= 0 else { return false }
        return dk_canvas_surface_move_layer(handle, UInt32(from), UInt32(to)) != 0
    }

 }
