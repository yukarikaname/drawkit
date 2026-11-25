// Brush files on iPad. An imported brush keeps its parameters; changing the picker afterwards
// intentionally resets them to the preset.

import SwiftUI
import UIKit

extension Studio {
    @discardableResult
    func importBrush(_ url: URL) -> Bool {
        let kind = url.path.withCString { dk_import_brush(handle, $0) }
        guard kind >= 0, let preset = BrushPreset(rawValue: Int(kind)) else {
            haptic(.error)
            return false
        }
        suppressBrushReset = true
        brushPreset = preset
        suppressBrushReset = false
        revision += 1
        haptic(.success)
        return true
    }
}
