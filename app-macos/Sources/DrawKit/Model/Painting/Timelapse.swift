// Timelapse: the stroke tape is the recording. Each video frame is the tape replayed at a timestamp,
// so the same file can later render at another size or frame rate. PNG frames are not kept: a PNG
// sequence would be gigabytes and would lose the layers, the timestamps and the vector strokes.
//
// AVFoundation does the encoding, so a Mac exports H.264 without ffmpeg and without a temporary
// image sequence.

import AVFoundation
import CoreVideo
import Foundation

enum Timelapse {
    struct Failure: LocalizedError {
        let message: String
        var errorDescription: String? { message }
    }

    /// Thirty frames a second, at most a minute of video. A long session takes bigger steps
    /// through the tape instead of producing a two-hour file.
    static func plan(durationMs: UInt64) -> (frames: Int, stepMs: UInt64) {
        let fps: UInt64 = 30
        let maxFrames = Int(fps * 60)
        let step = max(1_000 / fps, durationMs / UInt64(maxFrames))
        let frames = min(maxFrames + 1, max(1, Int(durationMs / step) + 1))
        return (frames, step)
    }

    static func write(
        handle: UnsafeMutableRawPointer,
        durationMs: UInt64,
        canvasWidth: Int,
        canvasHeight: Int,
        longEdge: Int,
        to url: URL,
        progress: (Double) -> Void
    ) async throws {
        let (frames, step) = plan(durationMs: durationMs)
        let size = videoSize(
            canvasWidth: canvasWidth, canvasHeight: canvasHeight,
            longEdge: longEdge)
        try? FileManager.default.removeItem(at: url)

        let writer: AVAssetWriter
        do {
            writer = try AVAssetWriter(outputURL: url, fileType: .mp4)
        } catch {
            throw Failure(message: "Could not start the video: \(error.localizedDescription)")
        }
        let input = AVAssetWriterInput(
            mediaType: .video,
            outputSettings: [
                AVVideoCodecKey: AVVideoCodecType.h264,
                AVVideoWidthKey: size.width,
                AVVideoHeightKey: size.height,
                AVVideoCompressionPropertiesKey: [
                    AVVideoAverageBitRateKey: max(6_000_000, size.width * size.height * 2),
                    AVVideoProfileLevelKey: AVVideoProfileLevelH264HighAutoLevel,
                ],
            ])
        let attributes = CVPixelBufferCreationAttributes(
            pixelFormatType: CVPixelFormatType(rawValue: kCVPixelFormatType_32BGRA),
            size: CVImageSize(width: size.width, height: size.height),
            compatibility: [],
            bytesPerRowAlignment: nil,
            planeAlignment: nil,
            extendedPixels: nil
        )
        let receiver = writer.inputPixelBufferReceiver(
            for: input, pixelBufferAttributes: attributes)
        do {
            try writer.start()
        } catch {
            throw Failure(message: "The video encoder failed: \(error.localizedDescription)")
        }
        writer.startSession(atSourceTime: .zero)
        guard let pool = receiver.pixelBufferPool else {
            throw Failure(message: "The video encoder made no pixel buffer pool.")
        }

        var frame = [UInt8](repeating: 0, count: size.width * size.height * 4)
        let replay = TapeReplay(handle: handle, width: canvasWidth, height: canvasHeight)
        var canvasFrame =
            replay == nil || replay?.isHdr == true
            ? []
            : [UInt8](repeating: 0, count: canvasWidth * canvasHeight * 4)
        var canvasLinear =
            replay?.isHdr == true
            ? [Float](repeating: 0, count: canvasWidth * canvasHeight * 4)
            : []
        for index in 0..<frames {
            let time = min(durationMs, UInt64(index) * step)
            var gpuDrawn = false
            if let replay, replay.isHdr, !canvasLinear.isEmpty {
                gpuDrawn = canvasLinear.withUnsafeMutableBufferPointer { buffer in
                    guard let base = buffer.baseAddress else { return false }
                    return replay.frameLinear(at: time, into: base)
                }
                if gpuDrawn {
                    blitLinear(
                        canvasLinear, canvasWidth, canvasHeight,
                        into: &frame, width: size.width, height: size.height)
                }
            } else if let replay, !canvasFrame.isEmpty {
                gpuDrawn = canvasFrame.withUnsafeMutableBufferPointer { buffer in
                    guard let base = buffer.baseAddress else { return false }
                    return replay.frame(at: time, into: base)
                }
                if gpuDrawn {
                    blit(
                        canvasFrame, canvasWidth, canvasHeight,
                        into: &frame, width: size.width, height: size.height)
                }
            }
            guard gpuDrawn else {
                throw Failure(message: "Could not render frame \(index) on the GPU.")
            }
            var pixelBuffer = try pool.makeMutablePixelBuffer()
            frame.withUnsafeBytes { source in
                pixelBuffer.accessUnsafeMutableRawPlaneBytes { planes in
                    guard let destination = planes.first else { return }
                    guard let bytes = destination.bytes.baseAddress,
                        let sourceBase = source.baseAddress
                    else { return }
                    let stride = destination.properties.bytesPerRow
                    let row = size.width * 4
                    for y in 0..<size.height {
                        memcpy(bytes + y * stride, sourceBase + y * row, row)
                    }
                }
            }
            try await receiver.append(
                CVReadOnlyPixelBuffer(pixelBuffer),
                with: CMTime(value: Int64(index), timescale: 30))
            progress(Double(index + 1) / Double(frames))
        }
        receiver.finish()
        await withCheckedContinuation { (continuation: CheckedContinuation<Void, Never>) in
            writer.finishWriting { continuation.resume() }
        }
        guard writer.status == .completed else {
            throw Failure(
                message: writer.error?.localizedDescription
                    ?? "The video was not finished.")
        }
    }

    /// A premultiplied frame over paper, resampled to the video size as BGRA.
    private static func blit(
        _ source: [UInt8], _ sourceWidth: Int, _ sourceHeight: Int,
        into target: inout [UInt8], width: Int, height: Int
    ) {
        guard sourceWidth > 0, sourceHeight > 0, width > 0, height > 0 else { return }
        source.withUnsafeBufferPointer { input in
            target.withUnsafeMutableBufferPointer { output in
                for y in 0..<height {
                    let sy = min(sourceHeight - 1, y * sourceHeight / height)
                    for x in 0..<width {
                        let sx = min(sourceWidth - 1, x * sourceWidth / width)
                        let at = (sy * sourceWidth + sx) * 4
                        let alpha = Float(input[at + 3]) / 255
                        let over = 1 - alpha
                        let red = min(1, Float(input[at]) / 255 + over)
                        let green = min(1, Float(input[at + 1]) / 255 + over)
                        let blue = min(1, Float(input[at + 2]) / 255 + over)
                        let to = (y * width + x) * 4
                        output[to] = UInt8((blue * 255).rounded())
                        output[to + 1] = UInt8((green * 255).rounded())
                        output[to + 2] = UInt8((red * 255).rounded())
                        output[to + 3] = 255
                    }
                }
            }
        }
    }

    /// A premultiplied linear frame over paper, resampled to the video size as BGRA.
    private static func blitLinear(
        _ source: [Float], _ sourceWidth: Int, _ sourceHeight: Int,
        into target: inout [UInt8], width: Int, height: Int
    ) {
        guard sourceWidth > 0, sourceHeight > 0, width > 0, height > 0 else { return }
        source.withUnsafeBufferPointer { input in
            target.withUnsafeMutableBufferPointer { output in
                for y in 0..<height {
                    let sy = min(sourceHeight - 1, y * sourceHeight / height)
                    for x in 0..<width {
                        let sx = min(sourceWidth - 1, x * sourceWidth / width)
                        let at = (sy * sourceWidth + sx) * 4
                        let over = 1 - input[at + 3]
                        let red = min(1, input[at] + over)
                        let green = min(1, input[at + 1] + over)
                        let blue = min(1, input[at + 2] + over)
                        let to = (y * width + x) * 4
                        output[to] = UInt8((blue * 255).rounded())
                        output[to + 1] = UInt8((green * 255).rounded())
                        output[to + 2] = UInt8((red * 255).rounded())
                        output[to + 3] = 255
                    }
                }
            }
        }
    }

    /// H.264 tops out at 4K on many encoders and wants even dimensions.
    private static func videoSize(
        canvasWidth: Int, canvasHeight: Int,
        longEdge: Int
    ) -> (width: Int, height: Int) {
        let cap = Double(min(max(longEdge, 320), 3840))
        let scale = min(1.0, cap / Double(max(canvasWidth, canvasHeight, 1)))
        func even(_ value: Int) -> Int {
            max(2, Int(Double(value) * scale) & ~1)
        }
        return (even(canvasWidth), even(canvasHeight))
    }
}
