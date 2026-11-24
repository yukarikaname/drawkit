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
        to url: URL,
        progress: (Double) -> Void
    ) async throws {
        let (frames, step) = plan(durationMs: durationMs)
        let size = videoSize(canvasWidth: canvasWidth, canvasHeight: canvasHeight)
        try? FileManager.default.removeItem(at: url)

        let writer: AVAssetWriter
        do {
            writer = try AVAssetWriter(outputURL: url, fileType: .mp4)
        } catch {
            throw Failure(message: "Could not start the video: \(error.localizedDescription)")
        }
        let input = AVAssetWriterInput(mediaType: .video, outputSettings: [
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
        for index in 0..<frames {
            let time = min(durationMs, UInt64(index) * step)
            let drawn = frame.withUnsafeMutableBytes { bytes -> Bool in
                guard let base = bytes.baseAddress?.assumingMemoryBound(to: UInt8.self) else {
                    return false
                }
                return dk_video_frame(handle, time, UInt32(size.width), UInt32(size.height), base) != 0
            }
            guard drawn else {
                throw Failure(message: "Could not render frame \(index).")
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
            throw Failure(message: writer.error?.localizedDescription
                ?? "The video was not finished.")
        }
    }

    /// H.264 tops out at 4K on many encoders and wants even dimensions.
    private static func videoSize(canvasWidth: Int, canvasHeight: Int) -> (width: Int, height: Int) {
        let scale = min(1.0, 3840.0 / Double(max(canvasWidth, canvasHeight, 1)))
        func even(_ value: Int) -> Int {
            max(2, Int(Double(value) * scale) & ~1)
        }
        return (even(canvasWidth), even(canvasHeight))
    }
}
