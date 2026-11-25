// Colour history and the hex label, shared with the colour sheet.

import SwiftUI
import UIKit

extension Studio {
    func sampleArtwork() {
        var values = [Float](repeating: 0, count: 24)
        let written = values.withUnsafeMutableBufferPointer {
            dk_palette(handle, 6, $0.baseAddress)
        }
        guard written > 0 else { return }
        for index in 0..<Int(written) {
            let colour = Color(.sRGB,
                               red: Double(values[index * 4]),
                               green: Double(values[index * 4 + 1]),
                               blue: Double(values[index * 4 + 2]),
                               opacity: Double(values[index * 4 + 3]))
            recent.removeAll { $0 == colour }
            recent.insert(colour, at: 0)
        }
        recent = Array(recent.prefix(10))
        if let first = recent.first { color = first }
    }

    func rememberColour() {
        if !recent.contains(where: { $0 == color }) {
            recent.insert(color, at: 0)
            recent = Array(recent.prefix(10))
        }
    }

    var hex: String {
        let ui = UIColor(color)
        var red: CGFloat = 0
        var green: CGFloat = 0
        var blue: CGFloat = 0
        var alpha: CGFloat = 1
        ui.getRed(&red, green: &green, blue: &blue, alpha: &alpha)
        return String(format: "#%02X%02X%02X",
                      Int(red * 255), Int(green * 255), Int(blue * 255))
    }

}
