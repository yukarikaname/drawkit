// The tool rail, on the left where Clip Studio puts it.
//
// Icons, not a segmented control: a painter reaches for the same three places every time, and a
// vertical strip keeps the canvas from being pushed down by a toolbar. The colour well sits at the
// bottom, also where CSP keeps it.

import SwiftUI

struct ToolRail: View {
    @Binding var tool: Tool
    let shortcuts: ShortcutStore
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    var body: some View {
        VStack(spacing: 8) {
            BrandMark(size: 24)
                .padding(.top, 2)
                .padding(.bottom, 4)
                .help("DrawKit")
            button(
                .brush, symbol: "paintbrush.pointed",
                help: "Brush (\(shortcuts.display(.brush)))")
            button(
                .eraser, symbol: "eraser",
                help: "Eraser (\(shortcuts.display(.eraser)))")
            button(
                .smudge, symbol: "drop.halffull",
                help: "Smudge (\(shortcuts.display(.smudge)))")
            button(
                .fill, symbol: "paintpalette.fill",
                help: "Fill (\(shortcuts.display(.fill)))")
            button(
                .eyedropper, symbol: "eyedropper",
                help: "Eyedropper (\(shortcuts.display(.eyedropper)))")
            button(
                .select, symbol: "rectangle.dashed",
                help: "Rectangular selection")
            Spacer()
        }
        .padding(.vertical, 14)
        .padding(.horizontal, 8)
        .frame(width: 68)
        .frame(maxHeight: .infinity)
        .proofStrip(ruleOn: .trailing)
    }

    private func button(_ value: Tool, symbol: String, help: String) -> some View {
        RailButton(symbol: symbol, selected: tool == value, help: help) {
            withAnimation(Motion.select.resolving(reduceMotion: reduceMotion)) {
                tool = value
            }
        }
    }
}

private struct RailButton: View {
    let symbol: String
    let selected: Bool
    let help: String
    let action: () -> Void

    @State private var hovering = false
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    var body: some View {
        Button(action: action) {
            Image(systemName: symbol)
                .font(.system(size: 17, weight: .medium))
                .foregroundStyle(selected ? Brand.graphite : Brand.chalk.opacity(0.82))
                .frame(width: 46, height: 46)
                .background {
                    // Selection is paper: the same surface the painting sits on, so the tool
                    // reads as the sheet's own furniture rather than a coloured control.
                    Rectangle()
                        .fill(selected ? Brand.proof : Color.clear)
                }
                .overlay {
                    Rectangle()
                        .strokeBorder(
                            selected ? Brand.chalk.opacity(0.45) : Color.clear,
                            lineWidth: 1)
                }
        }
        .buttonStyle(SpringPressStyle(reduceMotion: reduceMotion))
        .onHover { inside in
            withAnimation(
                reduceMotion
                    ? .easeOut(duration: 0.10)
                    : .easeOut(duration: 0.18)
            ) {
                hovering = inside
            }
        }
        .help(help)
    }
}
