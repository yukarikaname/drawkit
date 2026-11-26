// Extract the visible painting into a line layer with the embedded TEED model.

import Foundation

@MainActor
extension Studio {
    func extractLineArt() {
        materializeCanvas()
        guard dk_extract_lineart(handle, 0.55, 0.12) != 0 else {
            haptic(.error)
            return
        }
        refreshAll()
        refreshLayers()
        finishChange()
        haptic(.success)
    }
}
