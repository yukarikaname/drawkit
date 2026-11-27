// The short list on the welcome screen. Paths are enough for a non-sandboxed local app; a URL that
// no longer exists is dropped the next time the list is read. A small PNG per document gives the
// list a face without opening every file on launch.

import AppKit
import CryptoKit
import Foundation

enum RecentDocuments {
    private static let key = "dev.drawkit.recentDocuments"
    private static let limit = 12

    static func all() -> [URL] {
        let paths = UserDefaults.standard.stringArray(forKey: key) ?? []
        let urls = paths.map { URL(fileURLWithPath: $0) }
            .filter { FileManager.default.fileExists(atPath: $0.path) }
        if urls.count != paths.count {
            UserDefaults.standard.set(urls.map(\.path), forKey: key)
        }
        return urls
    }

    static func add(_ url: URL) {
        var paths = all().map(\.path)
        paths.removeAll { $0 == url.path }
        paths.insert(url.path, at: 0)
        UserDefaults.standard.set(Array(paths.prefix(limit)), forKey: key)
    }

    static func clear() {
        UserDefaults.standard.removeObject(forKey: key)
    }

    static func thumbnail(for url: URL) -> NSImage? {
        guard let data = try? Data(contentsOf: thumbnailURL(url)) else { return nil }
        return NSImage(data: data)
    }

    static func storeThumbnail(_ image: CGImage, for url: URL) {
        let size = NSSize(width: 192, height: 192)
        let thumbnail = NSImage(size: size, flipped: false) { rect in
            NSColor.white.setFill()
            rect.fill()
            let scale = min(
                rect.width / CGFloat(image.width),
                rect.height / CGFloat(image.height))
            let width = CGFloat(image.width) * scale
            let height = CGFloat(image.height) * scale
            let target = NSRect(
                x: (rect.width - width) / 2, y: (rect.height - height) / 2,
                width: width, height: height)
            NSImage(cgImage: image, size: NSSize(width: width, height: height)).draw(in: target)
            return true
        }
        guard let tiff = thumbnail.tiffRepresentation,
            let representation = NSBitmapImageRep(data: tiff),
            let png = representation.representation(using: .png, properties: [:])
        else { return }
        let target = thumbnailURL(url)
        try? FileManager.default.createDirectory(
            at: target.deletingLastPathComponent(),
            withIntermediateDirectories: true)
        try? png.write(to: target)
    }

    private static func thumbnailURL(_ url: URL) -> URL {
        let digest = SHA256.hash(data: Data(url.path.utf8))
            .map { String(format: "%02x", $0) }
            .joined()
        let base =
            FileManager.default.urls(
                for: .applicationSupportDirectory,
                in: .userDomainMask
            ).first
            ?? URL(fileURLWithPath: NSTemporaryDirectory())
        return base.appendingPathComponent("DrawKit/recent/\(digest).png")
    }
}
