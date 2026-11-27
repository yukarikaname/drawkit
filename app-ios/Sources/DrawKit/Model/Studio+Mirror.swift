// The mirrored copies of a segment, asked of the editor rather than the display surface.

import Foundation

extension Studio {
    /// Four floats per copy: from x, from y, to x, to y. The mirror angle and "both" flag live in
    /// the core, so this goes through the editor handle.
    func strokeVariants(
        fromX: Float, fromY: Float, toX: Float, toY: Float,
        _ body: (
            _ fromX: Float, _ fromY: Float,
            _ toX: Float, _ toY: Float
        ) -> Void
    ) {
        var buffer = [Float](repeating: 0, count: 16)
        let count = buffer.withUnsafeMutableBufferPointer {
            dk_mirror_segments(handle, fromX, fromY, toX, toY, $0.baseAddress)
        }
        guard count > 0 else {
            body(fromX, fromY, toX, toY)
            return
        }
        for index in 0..<Int(count) {
            body(
                buffer[index * 4], buffer[index * 4 + 1],
                buffer[index * 4 + 2], buffer[index * 4 + 3])
        }
    }
}
