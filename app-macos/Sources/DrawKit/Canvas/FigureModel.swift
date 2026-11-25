// The 3D reference figure: one GPU handle, a camera, and the image the canvas overlays.
//
// The renderer lives in the Rust GPU crate. A pose change rebuilds the mesh, renders it offscreen
// and reads the pixels back into a CGImage; a reference layer does not need a swapchain.

import SwiftUI
import AppKit
import Observation
import simd

@MainActor
@Observable
final class FigureModel {
    static let presetNames = ["Neutral", "Male", "Female", "Anime", "Chibi"]
    static let poseNames = ["A-pose", "T-pose", "Arms up", "Contrapposto"]
    static let draggableJoints: [Int32] = [8, 9, 12, 13, 15, 16, 19, 20]

    private(set) var image: CGImage?
    var visible = false {
        didSet { revision &+= 1 }
    }
    var opacity: Double = 0.55 {
        didSet { revision &+= 1 }
    }
    /// See PaintModel.revision: the canvas is an NSView, so a value input has to carry the change.
    private(set) var revision: UInt64 = 0
    var preset: Int = 0 {
        didSet {
            dk_figure_set_preset(handle, UInt32(preset))
            render()
        }
    }
    private(set) var yaw: Float = 0.35
    private(set) var pitch: Float = 0.08
    private(set) var distance: Float = 4.5

    // The three-light rig. Angles are degrees for the inspector; the core takes radians.
    var keyAzimuth: Double = -41 {
        didSet { applyLighting() }
    }
    var keyElevation: Double = 58 {
        didSet { applyLighting() }
    }
    var keyIntensity: Double = 0.7 {
        didSet { applyLighting() }
    }
    var keyWarmth: Double = 0 {
        didSet { applyLighting() }
    }
    var fillAzimuth: Double = 125 {
        didSet { applyLighting() }
    }
    var fillIntensity: Double = 0 {
        didSet { applyLighting() }
    }
    var fillWarmth: Double = 0 {
        didSet { applyLighting() }
    }
    var rimAzimuth: Double = 160 {
        didSet { applyLighting() }
    }
    var rimIntensity: Double = 0 {
        didSet { applyLighting() }
    }
    var rimWarmth: Double = 0 {
        didSet { applyLighting() }
    }
    var ambient: Double = 0.3 {
        didSet { applyLighting() }
    }

    let available: Bool
    nonisolated(unsafe) private let handle: UnsafeMutableRawPointer?
    private let width = 512
    private let height = 768
    @ObservationIgnored private var renderedPixels: [UInt8] = []
    @ObservationIgnored private var poseJoint: Int32 = -1
    @ObservationIgnored private var aimJoint: Int32 = -1
    @ObservationIgnored private var posePlane =
        (point: SIMD3<Float>(0, 0, 0), normal: SIMD3<Float>(0, 0, -1))
    @ObservationIgnored private var suppressLighting = false

    init() {
        handle = dk_figure_new(0)
        available = handle != nil
        applyLighting()
    }

    deinit { dk_figure_free(handle) }

    func orbit(dx: CGFloat, dy: CGFloat) {
        yaw += Float(dx) * 0.01
        pitch = min(max(pitch + Float(dy) * 0.01, -1.3), 1.3)
        render()
    }

    func zoom(by factor: Float) {
        distance = min(max(distance / factor, 1.8), 9.0)
        render()
    }

    func reset() {
        dk_figure_reset(handle)
        yaw = 0.35
        pitch = 0.08
        distance = 4.5
        render()
    }

    func resetLighting() {
        keyAzimuth = -41
        keyElevation = 58
        keyIntensity = 0.7
        keyWarmth = 0
        fillAzimuth = 125
        fillIntensity = 0
        fillWarmth = 0
        rimAzimuth = 160
        rimIntensity = 0
        rimWarmth = 0
        ambient = 0.3
    }

    /// Read the shading of the painting and propose a key light. The model is local and sees a
    /// grayscale composite; the painter can still adjust every slider afterwards.
    func estimateLighting(from model: PaintModel) {
        guard available else { return }
        _ = dk_composite(model.handle, model.pixels.pointer)
        var result = [Float](repeating: 0, count: 5)
        let read = result.withUnsafeMutableBufferPointer { values in
            dk_figure_estimate_lighting(model.pixels.pointer, UInt32(model.width),
                                        UInt32(model.height), values.baseAddress)
        }
        guard read != 0 else { return }
        let direction = simd_normalize(SIMD3<Float>(result[0], result[1], result[2]))
        suppressLighting = true
        keyAzimuth = Double(atan2(direction.x, direction.z)) * 180 / .pi
        keyElevation = Double(asin(max(-1, min(1, direction.y)))) * 180 / .pi
        keyIntensity = Double(result[3])
        ambient = Double(result[4])
        suppressLighting = false
        applyLighting()
    }

    private func applyLighting() {
        guard handle != nil, !suppressLighting else { return }
        let radians = Double.pi / 180
        _ = dk_figure_set_lighting(
            handle,
            Float(keyAzimuth * radians), Float(keyElevation * radians),
            Float(keyIntensity), Float(keyWarmth),
            Float(fillAzimuth * radians), Float(15 * radians),
            Float(fillIntensity), Float(fillWarmth),
            Float(rimAzimuth * radians), Float(10 * radians),
            Float(rimIntensity), Float(rimWarmth),
            Float(ambient))
        render()
    }

    func mirror() {
        dk_figure_mirror(handle)
        render()
    }

    func applyPose(_ index: Int) {
        dk_figure_pose(handle, UInt32(index))
        render()
    }

    // MARK: Posing

    /// Grab a hand or a foot under the pointer. Returns true when the drag should pose instead of
    /// orbit; it is the caller's job to keep the two gestures apart.
    func beginPose(at point: CGPoint, in rect: CGRect) -> Bool {
        guard handle != nil, rect.width > 0, let ray = ray(at: point, in: rect) else { return false }
        let joint = dk_figure_pick(handle, ray.origin.x, ray.origin.y, ray.origin.z,
                                   ray.direction.x, ray.direction.y, ray.direction.z, 0.06)
        guard joint >= 0, chain(for: joint) != nil || aimTarget(for: joint) != nil else {
            return false
        }
        var position = [Float](repeating: 0, count: 3)
        let read = position.withUnsafeMutableBufferPointer {
            dk_figure_joint(handle, UInt32(joint), $0.baseAddress)
        }
        guard read != 0 else { return false }
        poseJoint = joint
        aimJoint = aimTarget(for: joint) ?? -1
        posePlane = (SIMD3(position[0], position[1], position[2]), camera.forward)
        return true
    }

    /// Move the grabbed joint on the plane that faced the camera when it was grabbed. That keeps a
    /// drag a screen-space gesture instead of a depth guess.
    func dragPose(to point: CGPoint, in rect: CGRect) {
        guard poseJoint >= 0, let ray = ray(at: point, in: rect) else { return }
        let denominator = simd_dot(ray.direction, posePlane.normal)
        guard abs(denominator) > 1e-5 else { return }
        let distance = simd_dot(posePlane.point - ray.origin, posePlane.normal) / denominator
        guard distance > 0 else { return }
        let target = ray.origin + ray.direction * distance

        if aimJoint >= 0 {
            _ = dk_figure_aim(handle, UInt32(aimJoint), target.x, target.y, target.z)
            render()
            return
        }
        guard let chain = chain(for: poseJoint) else { return }
        var elbow = [Float](repeating: 0, count: 3)
        guard (elbow.withUnsafeMutableBufferPointer {
            dk_figure_joint(handle, UInt32(chain.lower), $0.baseAddress)
        }) != 0 else { return }
        let pole = SIMD3<Float>(elbow[0], elbow[1], elbow[2])
        _ = dk_figure_ik(handle, UInt32(chain.upper), UInt32(chain.lower), UInt32(chain.end),
                         target.x, target.y, target.z, pole.x, pole.y, pole.z)
        render()
    }

    /// Joint indices from `Joint` in the figure crate. Elbows and knees are dragged by rotating the
    /// bone above them; hands and feet are the ends a painter moves.
    private func aimTarget(for joint: Int32) -> Int32? {
        switch joint {
        case 8: return 7      // left elbow: rotate the upper arm
        case 12: return 11    // right elbow
        case 15: return 14    // left knee: rotate the thigh
        case 19: return 18    // right knee
        default: return nil
        }
    }

    private func chain(for joint: Int32) -> (upper: Int, lower: Int, end: Int)? {
        switch joint {
        case 9: return (7, 8, 9)      // left hand: upper arm, forearm, hand
        case 13: return (11, 12, 13)  // right hand
        case 16: return (14, 15, 16)  // left foot: thigh, shin, foot
        case 20: return (18, 19, 20)  // right foot
        default: return nil
        }
    }

    /// Bake the current pose into a new painting layer, at full opacity and in the same place the
    /// overlay occupies. The reference is hidden afterwards so the result is what the painter sees.
    func bake(into model: PaintModel) {
        guard available, let image, !renderedPixels.isEmpty else { return }
        let width = model.width
        let height = model.height
        var buffer = [UInt8](repeating: 0, count: width * height * 4)
        let drawn = buffer.withUnsafeMutableBytes { bytes -> Bool in
            guard let context = CGContext(
                data: bytes.baseAddress, width: width, height: height, bitsPerComponent: 8,
                bytesPerRow: width * 4, space: CGColorSpaceCreateDeviceRGB(),
                bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue) else { return false }
            context.interpolationQuality = .high
            let figureHeight = CGFloat(height) * 0.9
            let figureWidth = figureHeight * 2.0 / 3.0
            context.draw(image, in: CGRect(x: (CGFloat(width) - figureWidth) / 2,
                                           y: (CGFloat(height) - figureHeight) / 2,
                                           width: figureWidth, height: figureHeight))
            return true
        }
        guard drawn, model.addReferenceLayer(premultiplied: buffer, width: width, height: height)
        else { return }
        visible = false
    }

    /// Where a joint projects into the overlay rectangle, for drawing a handle that can be grabbed.
    func jointScreenPosition(_ joint: Int32, in rect: CGRect) -> CGPoint? {
        guard handle != nil, rect.width > 0 else { return nil }
        var position = [Float](repeating: 0, count: 3)
        let read = position.withUnsafeMutableBufferPointer {
            dk_figure_joint(handle, UInt32(joint), $0.baseAddress)
        }
        guard read != 0 else { return nil }
        let camera = self.camera
        let world = SIMD3<Float>(position[0], position[1], position[2])
        let relative = world - camera.eye
        let depth = simd_dot(relative, camera.forward)
        guard depth > 0.03 else { return nil }
        let x = simd_dot(relative, camera.right) / (depth * camera.tanHalf * camera.aspect)
        let y = simd_dot(relative, camera.up) / (depth * camera.tanHalf)
        return CGPoint(x: rect.minX + CGFloat(x * 0.5 + 0.5) * rect.width,
                       y: rect.minY + CGFloat(y * 0.5 + 0.5) * rect.height)
    }

    private struct ViewCamera {
        let eye: SIMD3<Float>
        let forward: SIMD3<Float>
        let right: SIMD3<Float>
        let up: SIMD3<Float>
        let tanHalf: Float
        let aspect: Float
    }

    private var camera: ViewCamera {
        let height = dk_figure_height(handle)
        let target = SIMD3<Float>(0, height * 0.55, 0)
        let eye = target + SIMD3<Float>(distance * cos(pitch) * sin(yaw),
                                        distance * sin(pitch),
                                        distance * cos(pitch) * cos(yaw))
        let forward = simd_normalize(target - eye)
        let right = simd_normalize(simd_cross(forward, SIMD3<Float>(0, 1, 0)))
        let up = simd_cross(right, forward)
        return ViewCamera(eye: eye, forward: forward, right: right, up: up,
                          tanHalf: tan(45 * .pi / 180 / 2),
                          aspect: Float(width) / Float(height))
    }

    private func ray(at point: CGPoint, in rect: CGRect) -> (origin: SIMD3<Float>,
                                                            direction: SIMD3<Float>)? {
        guard rect.width > 0, rect.height > 0 else { return nil }
        let camera = self.camera
        let x = Float((point.x - rect.minX) / rect.width) * 2 - 1
        let y = Float((point.y - rect.minY) / rect.height) * 2 - 1
        let direction = simd_normalize(camera.forward
            + camera.right * (x * camera.tanHalf * camera.aspect)
            + camera.up * (y * camera.tanHalf))
        return (camera.eye, direction)
    }

    func render() {
        guard handle != nil else { return }
        var pixels = [UInt8](repeating: 0, count: width * height * 4)
        let drawn = pixels.withUnsafeMutableBytes {
            dk_figure_render(handle, UInt32(width), UInt32(height), yaw, pitch, distance,
                             $0.bindMemory(to: UInt8.self).baseAddress)
        }
        guard drawn != 0, let provider = CGDataProvider(data: Data(pixels) as CFData) else { return }
        renderedPixels = pixels
        revision &+= 1
        image = CGImage(
            width: width, height: height, bitsPerComponent: 8, bitsPerPixel: 32,
            bytesPerRow: width * 4, space: CGColorSpaceCreateDeviceRGB(),
            bitmapInfo: CGBitmapInfo(rawValue: CGImageAlphaInfo.premultipliedLast.rawValue),
            provider: provider, decode: nil, shouldInterpolate: true, intent: .defaultIntent)
    }
}
