// The two automations Shortcuts can run against the front painting. They live in the main target:
// App Intents are discovered from the app bundle's metadata, and a separate framework target would
// only move the same code one level away from the document it needs.
//
// `perform()` is not main-actor isolated, while the document is, so every touch of the model hops
// through `MainActor.run`. The dialog strings explain the refusal instead of failing silently; a
// Shortcut that runs while the welcome screen is up should say why nothing happened.

import AppIntents
import Foundation

enum DrawKitIntentError: Error, CustomLocalizedStringResourceConvertible {
    case noDocument
    case sizeOutOfRange
    case invalidPath
    case exportFailed

    var localizedStringResource: LocalizedStringResource {
        switch self {
        case .noDocument:
            "Open or create a DrawKit painting first."
        case .sizeOutOfRange:
            "A canvas must be between 16 and 8192 pixels on each side."
        case .invalidPath:
            "Give a PNG file path inside a folder that already exists."
        case .exportFailed:
            "DrawKit could not write the PNG. Check the folder permissions and free space."
        }
    }
}

struct CreateDocumentIntent: AppIntent {
    static let title: LocalizedStringResource = "Create Painting"
    static let description = IntentDescription("Create a new DrawKit canvas at a chosen size.")
    static let supportedModes: IntentModes = .foreground

    @Parameter(title: "Width", description: "Canvas width in pixels.", default: 1024)
    var width: Int

    @Parameter(title: "Height", description: "Canvas height in pixels.", default: 1024)
    var height: Int

    static var parameterSummary: some ParameterSummary {
        Summary("Create a \(\.$width) × \(\.$height) painting")
    }

    func perform() async throws -> some IntentResult & ProvidesDialog {
        guard (16...8192).contains(width), (16...8192).contains(height) else {
            throw DrawKitIntentError.sizeOutOfRange
        }
        guard let model = await PaintModel.current else {
            throw DrawKitIntentError.noDocument
        }
        await MainActor.run {
            model.reset(width: width, height: height)
        }
        return .result(dialog: "Created a \(width) × \(height) painting.")
    }
}

struct ExportPNGIntent: AppIntent {
    static let title: LocalizedStringResource = "Export Painting as PNG"
    static let description = IntentDescription("Write the front DrawKit painting to a PNG file.")
    static let supportedModes: IntentModes = .foreground

    @Parameter(title: "Path", description: "The PNG file to write; a folder gets DrawKit.png.")
    var path: String

    static var parameterSummary: some ParameterSummary {
        Summary("Export the painting to \(\.$path)")
    }

    func perform() async throws -> some IntentResult & ProvidesDialog {
        guard let model = await PaintModel.current,
              await MainActor.run(body: { model.hasDocument }) else {
            throw DrawKitIntentError.noDocument
        }
        let trimmed = path.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { throw DrawKitIntentError.invalidPath }
        var url = URL(fileURLWithPath: (trimmed as NSString).expandingTildeInPath)
        if url.hasDirectoryPath || FileManager.default.isDirectory(url) {
            url.appendPathComponent("DrawKit.png")
        } else if url.pathExtension.isEmpty {
            url.appendPathExtension("png")
        }
        let folder = url.deletingLastPathComponent()
        guard FileManager.default.fileExists(atPath: folder.path) else {
            throw DrawKitIntentError.invalidPath
        }
        let exported = await MainActor.run {
            model.exportPNG(to: url)
        }
        guard exported else { throw DrawKitIntentError.exportFailed }
        return .result(dialog: "Exported \(url.lastPathComponent).")
    }
}

private extension FileManager {
    /// `fileExists` alone is true for files and folders alike; the folder check matters because a
    /// path like `~/Pictures` should gain a file name, not become a file called Pictures.
    func isDirectory(_ url: URL) -> Bool {
        var isDirectory: ObjCBool = false
        return fileExists(atPath: url.path, isDirectory: &isDirectory) && isDirectory.boolValue
    }
}
