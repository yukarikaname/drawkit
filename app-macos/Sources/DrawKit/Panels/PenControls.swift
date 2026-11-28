// Pen-sized wrappers around the native controls.
//
// The controls stay AppKit/SwiftUI sliders, switches and steppers, so they keep the system's
// Liquid Glass material and, in the switch's case, its spring. What changes is the control size
// and the accent: the slider is `.large` for a pen, while switches, steppers and checkboxes stay
// `.regular` so they do not tower over the 12 pt labels around them. The tint is the app's one
// vermilion once the system has drawn the control.

import SwiftUI

struct PenSlider: View {
    @Binding var value: Double
    let range: ClosedRange<Double>
    var step: Double?
    var width: CGFloat = 140
    var accent: Color = Brand.chalk

    var body: some View {
        Group {
            if let step {
                Slider(value: $value, in: range, step: step)
            } else {
                Slider(value: $value, in: range)
            }
        }
        .controlSize(.large)
        .tint(accent)
        .frame(width: width, height: 32)
        .accessibilityValue(Text("\(value)"))
    }
}

struct PenStepper: View {
    @Binding var value: Double
    let range: ClosedRange<Double>
    var step: Double = 1

    var body: some View {
        Stepper("", value: $value, in: range, step: step)
            .labelsHidden()
            .controlSize(.regular)
            .tint(Brand.chalk)
    }
}

struct PenToggle: View {
    @Binding var isOn: Bool
    var help: String = ""

    var body: some View {
        Toggle("", isOn: $isOn)
            .labelsHidden()
            .toggleStyle(.switch)
            .controlSize(.regular)
            .tint(Brand.chalk)
            .help(help)
    }
}

struct PenCheck: View {
    let title: String
    @Binding var isOn: Bool

    var body: some View {
        Toggle(title, isOn: $isOn)
            .toggleStyle(.checkbox)
            .controlSize(.regular)
            .tint(Brand.chalk)
    }
}
