// Smudge segments go straight into the resident tiles; the CPU copy catches up later.

import AppKit

extension CanvasView {
    /// One smudge segment straight into the resident tiles. The CPU records the command in
    /// deferred mode; this pass moves the pixels and the journal groups every segment into one
    /// undo step.
    func smudgeSegment(surface: PaintSurface, model: PaintModel,
                               from: CGPoint, to: CGPoint, pressure: CGFloat) {
        let params = model.brushParameters
        let p = Float(pressure)
        let pressureRadius = Float(params[2])
        let painted = Float(radius) * (pressureRadius + (1 - pressureRadius) * p)
        surface.strokeBegin(
            width: model.width, height: model.height,
            color: (0, 0, 0, 1), flow: Float(smudgeStrength), pressureAlpha: 0,
            paper: 0, tip: 0, dual: 0, dualScale: 1,
            dualHardness: 0, dualRoundness: 1, dualAngle: 0, dualPaper: 0, dualTip: 0,
            wet: 0, jitter: 0, grain: 0, dualJitter: 0, dualGrain: 0,
            seed: 0, erase: false, previewLayer: .max)
        surface.strokeSegment(
            fromX: Float(from.x), fromY: Float(from.y),
            toX: Float(to.x), toY: Float(to.y),
            radius: painted, hardness: Float(params[0]), spacing: 0.25,
            roundness: 1, angle: 0, pressure: 1)
        surface.setStrokeSmudge(dx: Float(from.x - to.x), dy: Float(from.y - to.y))
        _ = surface.strokeCommit(layer: model.activeLayer, color: (0, 0, 0), flow: 1)
        surface.strokeEnd()
    }
}
