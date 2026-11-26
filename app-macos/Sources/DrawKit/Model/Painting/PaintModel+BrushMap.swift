// Paper and tip map uploads, and the seed the GPU jitter uses.

import Foundation

extension PaintModel {
    /// Upload the imported paper/tip map the next stroke samples, or clear it.
    func uploadBrushMap(_ surface: PaintSurface) {
        var size = [UInt32](repeating: 0, count: 2)
        let found = size.withUnsafeMutableBufferPointer {
            dk_brush_texture_size(handle, $0.baseAddress)
        }
        guard found != 0, size[0] > 0, size[1] > 0 else {
            surface.clearBrushMap()
            return
        }
        var data = [UInt8](repeating: 0, count: Int(size[0]) * Int(size[1]))
        let read = data.withUnsafeMutableBufferPointer {
            dk_brush_texture_preview(handle, size[0], size[1], $0.baseAddress)
        }
        if read != 0 {
            surface.setBrushMap(width: Int(size[0]), height: Int(size[1]), data: data)
        } else {
            surface.clearBrushMap()
        }
    }
}
