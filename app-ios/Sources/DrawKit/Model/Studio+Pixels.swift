// The live CGImage over the core buffer and the small change bookkeeping.

import CoreGraphics
import SwiftUI
import UIKit

extension Studio {
    func makeImage() -> CGImage? {
        let provider = CGDataProvider(
            dataInfo: nil,
            data: UnsafeRawPointer(buffer),
            size: width * height * 4,
            releaseData: { _, _, _ in }
        )
        guard let provider else { return nil }
        return CGImage(
            width: width, height: height, bitsPerComponent: 8, bitsPerPixel: 32,
            bytesPerRow: bytesPerRow, space: CGColorSpaceCreateDeviceRGB(),
            bitmapInfo: CGBitmapInfo(rawValue: CGImageAlphaInfo.premultipliedLast.rawValue),
            provider: provider, decode: nil, shouldInterpolate: false, intent: .defaultIntent
        )
    }

    func finishChange() {
        canUndo = dk_can_undo(handle) != 0
        canRedo = dk_can_redo(handle) != 0
        isBlank = dk_is_clear(handle) != 0
        revision += 1
    }

    func stampTime() {
        let now = UInt64(CACurrentMediaTime() * 1000)
        lastTime = max(lastTime + 1, now)
    }

    func haptic(_ type: UINotificationFeedbackGenerator.FeedbackType) {
        MainActor.assumeIsolated {
            UINotificationFeedbackGenerator().notificationOccurred(type)
        }
    }

    func colorComponents() -> (Float, Float, Float) {
        let ui = UIColor(color)
        var red: CGFloat = 0
        var green: CGFloat = 0
        var blue: CGFloat = 0
        var alpha: CGFloat = 1
        ui.getRed(&red, green: &green, blue: &blue, alpha: &alpha)
        let gain = Float(pow(2.0, hdr ? max(glow, 0) : 0))
        return (Float(red) * gain, Float(green) * gain, Float(blue) * gain)
    }
}
