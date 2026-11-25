import AppKit

// The AppKit bridge for the shared colour-space type.
extension PaintColorSpace {
    var nsColorSpace: NSColorSpace {
        switch self {
        case .sRGB: .sRGB
        case .displayP3: .displayP3
        case .adobeRgb: .adobeRGB1998
        }
    }
}
