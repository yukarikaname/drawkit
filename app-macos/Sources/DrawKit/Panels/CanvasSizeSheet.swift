// Crop or pad the canvas. The art is never scaled; the anchor decides which side keeps the pixels
// and which side gains or loses them.

import SwiftUI

enum CanvasAnchor: CaseIterable {
    case topLeft, top, topRight, left, center, right, bottomLeft, bottom, bottomRight

    var x: Double {
        switch self {
        case .topLeft, .left, .bottomLeft: 0
        case .top, .center, .bottom: 0.5
        default: 1
        }
    }

    var y: Double {
        switch self {
        case .topLeft, .top, .topRight: 0
        case .left, .center, .right: 0.5
        default: 1
        }
    }
}

struct CanvasSizeSheet: View {
    let model: PaintModel
    let onResize: (Int, Int, Double, Double) -> Bool
    @Environment(\.dismiss) private var dismiss
    @State private var widthText: String
    @State private var heightText: String
    @State private var anchor = CanvasAnchor.center
    @State private var failure: String?

    private let rows: [[CanvasAnchor]] = [
        [.topLeft, .top, .topRight],
        [.left, .center, .right],
        [.bottomLeft, .bottom, .bottomRight],
    ]

    init(model: PaintModel, onResize: @escaping (Int, Int, Double, Double) -> Bool) {
        self.model = model
        self.onResize = onResize
        _widthText = State(initialValue: "\(model.width)")
        _heightText = State(initialValue: "\(model.height)")
    }

    private var requested: (width: Int, height: Int)? {
        guard let width = Int(widthText), let height = Int(heightText),
              (16...8192).contains(width), (16...8192).contains(height) else { return nil }
        return (width, height)
    }

    private var detail: String {
        guard let (width, height) = requested else { return "16 – 8192 px" }
        let dx = Int((Double(width - model.width) * anchor.x).rounded())
        let dy = Int((Double(height - model.height) * anchor.y).rounded())
        func side(_ amount: Int, _ axis: String) -> String {
            amount >= 0 ? "pad \(amount) \(axis)" : "crop \(-amount) \(axis)"
        }
        return "\(width) × \(height) px · \(side(dx, "x")) · \(side(dy, "y"))"
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            VStack(alignment: .leading, spacing: 3) {
                Text("Canvas Size").font(.headline)
                Text("Current \(model.width) × \(model.height) px")
                    .font(.caption).foregroundStyle(Brand.muted)
            }
            HStack(spacing: 8) {
                numberField("Width", text: $widthText)
                Text("×").foregroundStyle(Brand.muted)
                numberField("Height", text: $heightText)
            }
            HStack(alignment: .top, spacing: 20) {
                VStack(alignment: .leading, spacing: 6) {
                    Text("Anchor").font(.caption).foregroundStyle(Brand.muted)
                    // The nine buttons are the document's corners and edges, not text flow: the
                    // top-left button must stay at the top left on screen in RTL too.
                    VStack(spacing: 4) {
                        ForEach(rows.indices, id: \.self) { row in
                            HStack(spacing: 4) {
                                ForEach(rows[row].indices, id: \.self) { column in
                                    anchorButton(rows[row][column])
                                }
                            }
                        }
                    }
                    .environment(\.layoutDirection, .leftToRight)
                }
                VStack(alignment: .leading, spacing: 8) {
                    Text(detail).font(.caption).foregroundStyle(Brand.muted)
                    Text("Resizing starts a new undo baseline.")
                        .font(.caption2).foregroundStyle(Brand.muted.opacity(0.8))
                    if let failure {
                        Text(failure).font(.caption).foregroundStyle(Brand.vermilion)
                    }
                }
            }
            HStack {
                Spacer()
                Button("Cancel") { dismiss() }.keyboardShortcut(.cancelAction)
                Button("Resize") { resize() }
                    .keyboardShortcut(.defaultAction)
                    .disabled(requested == nil)
            }
        }
        .padding(20)
        .frame(width: 430)
        .background(Brand.carbon)
        .preferredColorScheme(.dark)
    }

    private func numberField(_ title: String, text: Binding<String>) -> some View {
        HStack(spacing: 5) {
            TextField(title, text: text)
                .textFieldStyle(.roundedBorder)
                .multilineTextAlignment(.trailing)
                .frame(width: 92)
            Text("px").font(.caption).foregroundStyle(Brand.muted)
        }
    }

    private func anchorButton(_ value: CanvasAnchor) -> some View {
        Button {
            anchor = value
        } label: {
            RoundedRectangle(cornerRadius: 5, style: .continuous)
                .fill(anchor == value ? Brand.vermilion : Brand.raised)
                .frame(width: 27, height: 27)
                .overlay(
                    Circle()
                        .fill(Brand.paper.opacity(anchor == value ? 0.95 : 0.32))
                        .frame(width: 6, height: 6)
                )
        }
        .buttonStyle(.plain)
        .help("Keep the art at this corner")
    }

    private func resize() {
        guard let (width, height) = requested else { return }
        if onResize(width, height, anchor.x, anchor.y) {
            dismiss()
        } else {
            failure = "Could not resize the canvas."
        }
    }
}
