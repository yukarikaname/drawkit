// A PencilKit scratch sheet. PencilKit owns the stroke smoothing and the tool picker; when the
// sketch is done it is rasterised once into a new painting layer and the core takes over.

import PencilKit
import SwiftUI

struct PencilKitSheet: View {
    let studio: Studio
    @Environment(\.dismiss) private var dismiss
    @State private var drawing = PKDrawing()

    var body: some View {
        VStack(spacing: 14) {
            HStack(spacing: 10) {
                BrandMark(size: 24, line: Brand.paper)
                Text("PencilKit sketch")
                    .font(.system(size: 22, weight: .bold, design: .rounded))
                    .foregroundStyle(Brand.paper)
                Spacer()
                Button("Cancel") { dismiss() }
                    .buttonStyle(.bordered)
                    .tint(Brand.paper)
                Button("Add layer") {
                    if studio.addPencilKitDrawing(drawing) {
                        dismiss()
                    }
                }
                .buttonStyle(.borderedProminent)
                .tint(Brand.vermilion)
                .disabled(drawing.bounds.isEmpty)
            }

            PencilKitCanvas(drawing: $drawing)
                .clipShape(RoundedRectangle(cornerRadius: 14))
                .overlay(
                    RoundedRectangle(cornerRadius: 14)
                        .stroke(Color.white.opacity(0.12), lineWidth: 1))
        }
        .padding(16)
        .background(Brand.carbon)
        .preferredColorScheme(.dark)
        .presentationDetents([.large])
        .presentationDragIndicator(.visible)
    }
}

private struct PencilKitCanvas: UIViewRepresentable {
    @Binding var drawing: PKDrawing

    func makeUIView(context: Context) -> PKCanvasView {
        let canvas = PKCanvasView()
        canvas.delegate = context.coordinator
        canvas.drawingPolicy = .anyInput
        canvas.backgroundColor = .white
        canvas.isOpaque = true
        canvas.tool = PKInkingTool(.pen, color: .black, width: 6)
        DispatchQueue.main.async {
            let picker = PKToolPicker()
            context.coordinator.picker = picker
            picker.addObserver(canvas)
            picker.setVisible(true, forFirstResponder: canvas)
            canvas.becomeFirstResponder()
        }
        return canvas
    }

    func updateUIView(_ canvas: PKCanvasView, context: Context) {
        if canvas.drawing != drawing {
            canvas.drawing = drawing
        }
    }

    func makeCoordinator() -> Coordinator {
        Coordinator(parent: self)
    }

    final class Coordinator: NSObject, PKCanvasViewDelegate {
        var parent: PencilKitCanvas
        var picker: PKToolPicker?

        init(parent: PencilKitCanvas) {
            self.parent = parent
        }

        func canvasViewDrawingDidChange(_ canvasView: PKCanvasView) {
            parent.drawing = canvasView.drawing
        }
    }
}
