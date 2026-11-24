// The document's working RGB space. The core stores encoded coordinates; the shell tags the
// canvas, picker images and export with the matching profile. sRGB is the safe default.
//
// Shared by the macOS window and the iPad view. AppKit's `NSColorSpace` bridge lives in the macOS
// target so this file compiles everywhere.

import CoreGraphics

enum PaintColorSpace: Int, CaseIterable, Identifiable {
    case sRGB = 0
    case displayP3 = 1
    case adobeRgb = 2

    var id: Self { self }

    var title: String {
        switch self {
        case .sRGB: "sRGB"
        case .displayP3: "Display P3"
        case .adobeRgb: "Adobe RGB"
        }
    }

    var cgColorSpace: CGColorSpace {
        switch self {
        case .sRGB:
            CGColorSpace(name: CGColorSpace.sRGB) ?? CGColorSpaceCreateDeviceRGB()
        case .displayP3:
            CGColorSpace(name: CGColorSpace.displayP3) ?? CGColorSpaceCreateDeviceRGB()
        case .adobeRgb:
            CGColorSpace(name: CGColorSpace.adobeRGB1998) ?? CGColorSpaceCreateDeviceRGB()
        }
    }
}
