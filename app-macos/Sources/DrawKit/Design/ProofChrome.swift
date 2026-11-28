// The proof chrome: chrome is furniture, so it is a flat slate plate bounded by a hairline,
// never a floating card with a drop shadow. Selection is the sheet's own paper.

import SwiftUI

extension View {
    /// A chrome plate: flat slate, one hairline border, square corners.
    func proofPlate() -> some View {
        background(Brand.slate)
            .overlay(Rectangle().strokeBorder(Brand.rule, lineWidth: 1))
    }

    /// A chrome strip hugging an edge, with the hairline on `ruleOn`.
    func proofStrip(ruleOn: Alignment) -> some View {
        background(Brand.slate)
            .overlay(alignment: ruleOn) {
                Rectangle()
                    .fill(Brand.rule)
                    .frame(width: 1)
            }
    }
}
