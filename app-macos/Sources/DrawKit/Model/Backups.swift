// Versioned copies of the last saves. Autosave protects against a crash; this protects against the
// painter saving over something they wanted and noticing a day later.

import Foundation

enum Backups {
    private static var directory: URL {
        let base = FileManager.default.urls(for: .applicationSupportDirectory,
                                            in: .userDomainMask).first
            ?? FileManager.default.temporaryDirectory
        return base.appendingPathComponent("DrawKit/backups", isDirectory: true)
    }

    /// Copy the file that is about to be overwritten and keep the newest ten.
    static func archive(_ url: URL) {
        guard FileManager.default.fileExists(atPath: url.path) else { return }
        let folder = directory.appendingPathComponent(
            url.deletingPathExtension().lastPathComponent, isDirectory: true)
        try? FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        let stamp = ISO8601DateFormatter().string(from: Date())
            .replacingOccurrences(of: ":", with: "-")
        let copy = folder.appendingPathComponent("\(stamp).\(url.pathExtension)")
        try? FileManager.default.copyItem(at: url, to: copy)
        prune(folder)
    }

    private static func prune(_ folder: URL) {
        let keys: [URLResourceKey] = [.contentModificationDateKey]
        guard let files = try? FileManager.default.contentsOfDirectory(
            at: folder, includingPropertiesForKeys: keys, options: .skipsHiddenFiles)
        else { return }
        let sorted = files.sorted {
            let left = (try? $0.resourceValues(forKeys: Set(keys)).contentModificationDate) ?? .distantPast
            let right = (try? $1.resourceValues(forKeys: Set(keys)).contentModificationDate) ?? .distantPast
            return left > right
        }
        for file in sorted.dropFirst(10) {
            try? FileManager.default.removeItem(at: file)
        }
    }
}
