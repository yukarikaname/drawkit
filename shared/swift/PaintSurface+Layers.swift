// Incremental resident-layer edits. A structural change moves hosts; it does not re-upload the
// layers it did not touch.

import Foundation

@_silgen_name("dk_canvas_surface_missing_rects")
private func dk_canvas_surface_missing_rects(
    _ handle: UnsafeMutableRawPointer?, _ out: UnsafeMutablePointer<UInt32>?,
    _ capacity: UInt32, _ count: UnsafeMutablePointer<UInt32>?) -> Int32

@_silgen_name("dk_canvas_surface_set_journal_budget")
private func dk_canvas_surface_set_journal_budget(
    _ handle: UnsafeMutableRawPointer?, _ bytes: UInt64)

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

extension PaintSurface {
    /// Tiles the resident budget evicted. The core surface still holds them, so the shell uploads
    /// these rectangles before the next present. At most `limit` are returned per call.
    func missingRects(limit: Int = 64) -> [(layer: Int, x: Int, y: Int, width: Int, height: Int)] {
        guard let handle else { return [] }
        var count: UInt32 = 0
        _ = dk_canvas_surface_missing_rects(handle, nil, 0, &count)
        guard count > 0 else { return [] }
        let wanted = min(Int(count), max(1, limit))
        var out = [UInt32](repeating: 0, count: wanted * 5)
        var written: UInt32 = 0
        let packed = out.withUnsafeMutableBufferPointer {
            dk_canvas_surface_missing_rects(handle, $0.baseAddress, UInt32(wanted), &written)
        }
        guard packed != 0 else { return [] }
        return (0..<Int(written)).map { index in
            let at = index * 5
            return (Int(out[at]), Int(out[at + 1]), Int(out[at + 2]),
                    Int(out[at + 3]), Int(out[at + 4]))
        }
    }

    /// The tile journal's undo budget. Lowering it drops old undo textures, so only do that once
    /// the core history is trimmed to match.
    func setJournalBudget(bytes: Int) {
        guard let handle else { return }
        dk_canvas_surface_set_journal_budget(handle, UInt64(max(0, bytes)))
    }
}
