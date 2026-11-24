// Crash recovery. An autosave is one project file plus a small record of where it came from, kept
// outside the document folder so a failed save cannot damage the painting the person chose.

import Foundation

struct AutosaveRecord: Codable, Identifiable {
    let savedAt: Date
    let source: String?
    let width: Int
    let height: Int

    var id: Date { savedAt }

    var sourceName: String {
        guard let source, !source.isEmpty else { return "Untitled" }
        return URL(fileURLWithPath: source).lastPathComponent
    }
}

enum Autosave {
    static var directory: URL {
        let base = FileManager.default.urls(for: .applicationSupportDirectory,
                                            in: .userDomainMask).first
            ?? FileManager.default.temporaryDirectory
        return base.appendingPathComponent("DrawKit/autosave", isDirectory: true)
    }

    static var documentURL: URL { directory.appendingPathComponent("recovery.dkp") }
    static var recordURL: URL { directory.appendingPathComponent("recovery.json") }

    /// The recovery copy, when one exists and its project file is still readable.
    static func pending() -> AutosaveRecord? {
        guard FileManager.default.fileExists(atPath: documentURL.path),
              let data = try? Data(contentsOf: recordURL),
              let record = try? JSONDecoder().decode(AutosaveRecord.self, from: data)
        else { return nil }
        return record
    }

    static func clear() {
        try? FileManager.default.removeItem(at: documentURL)
        try? FileManager.default.removeItem(at: recordURL)
    }
}
