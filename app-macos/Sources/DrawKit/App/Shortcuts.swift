// Custom keyboard shortcuts with presets for the apps painters already know.
//
// The store owns the mapping, persists it, and is the only place that turns a key into an action.
// Menus read the same mapping; the canvas uses it for the single-key tool shortcuts. A preset is a
// starting point, not a lock: editing one key keeps the bindings and lets the painter reset later.

import AppKit
import Observation
import SwiftUI

enum ShortcutScope {
    case canvas
    case application
}

enum ShortcutAction: String, CaseIterable, Identifiable, Codable {
    case brush
    case eraser
    case smudge
    case fill
    case eyedropper
    case fit
    case smaller
    case larger
    case undo
    case redo
    case open
    case save
    case export
    case clearLayer

    var id: String { rawValue }

    var title: LocalizedStringResource {
        switch self {
        case .brush: return "Brush"
        case .eraser: return "Eraser"
        case .smudge: return "Smudge"
        case .fill: return "Fill"
        case .eyedropper: return "Eyedropper"
        case .fit: return "Fit canvas"
        case .smaller: return "Smaller brush"
        case .larger: return "Larger brush"
        case .undo: return "Undo"
        case .redo: return "Redo"
        case .open: return "Open project"
        case .save: return "Save project"
        case .export: return "Export PNG"
        case .clearLayer: return "Clear layer"
        }
    }

    var scope: ShortcutScope {
        switch self {
        case .brush, .eraser, .fill, .eyedropper, .fit, .smaller, .larger:
            return .canvas
        default:
            return .application
        }
    }

    static var canvasActions: [ShortcutAction] {
        allCases.filter { $0.scope == .canvas }
    }

    static var applicationActions: [ShortcutAction] {
        allCases.filter { $0.scope == .application }
    }
}

struct ShortcutModifiers: OptionSet, Hashable, Codable {
    let rawValue: Int

    static let command = ShortcutModifiers(rawValue: 1 << 0)
    static let shift = ShortcutModifiers(rawValue: 1 << 1)
    static let option = ShortcutModifiers(rawValue: 1 << 2)
    static let control = ShortcutModifiers(rawValue: 1 << 3)

    init(rawValue: Int) {
        self.rawValue = rawValue
    }

    init(eventFlags: NSEvent.ModifierFlags) {
        var value: ShortcutModifiers = []
        if eventFlags.contains(.command) { value.insert(.command) }
        if eventFlags.contains(.shift) { value.insert(.shift) }
        if eventFlags.contains(.option) { value.insert(.option) }
        if eventFlags.contains(.control) { value.insert(.control) }
        self = value
    }

    var display: String {
        var text = ""
        if contains(.control) { text += "⌃" }
        if contains(.option) { text += "⌥" }
        if contains(.shift) { text += "⇧" }
        if contains(.command) { text += "⌘" }
        return text
    }

    var eventModifiers: EventModifiers {
        var value: EventModifiers = []
        if contains(.command) { value.insert(.command) }
        if contains(.shift) { value.insert(.shift) }
        if contains(.option) { value.insert(.option) }
        if contains(.control) { value.insert(.control) }
        return value
    }

    static let choices: [(label: String, modifier: ShortcutModifiers)] = [
        ("⌘", .command),
        ("⇧", .shift),
        ("⌥", .option),
        ("⌃", .control),
    ]

    init(from decoder: Decoder) throws {
        self.init(rawValue: try decoder.singleValueContainer().decode(Int.self))
    }

    func encode(to encoder: Encoder) throws {
        var container = encoder.singleValueContainer()
        try container.encode(rawValue)
    }
}

struct KeySpec: Codable, Hashable {
    var key: String
    var modifiers: ShortcutModifiers = []

    static func letter(_ key: String, _ modifiers: ShortcutModifiers = []) -> KeySpec {
        KeySpec(key: key.lowercased(), modifiers: modifiers)
    }

    var display: String {
        let label = key.count == 1 && key.first?.isLetter == true ? key.uppercased() : key
        return modifiers.display + label
    }

    var keyEquivalent: KeyEquivalent {
        KeyEquivalent(key.first ?? "?")
    }
}

enum ShortcutPreset: String, CaseIterable, Identifiable, Codable {
    case drawKit
    case clipStudio
    case photoshop
    case procreate

    var id: String { rawValue }

    var title: LocalizedStringResource {
        switch self {
        case .drawKit: return "DrawKit"
        case .clipStudio: return "Clip Studio Paint"
        case .photoshop: return "Photoshop"
        case .procreate: return "Procreate"
        }
    }

    /// Only the actions this app has are mapped. A tool the source app has and we do not is simply
    /// absent, not faked with a key that does nothing.
    var bindings: [ShortcutAction: KeySpec] {
        var map: [ShortcutAction: KeySpec] = [
            .brush: .letter("b"),
            .eraser: .letter("e"),
            .smudge: .letter("s"),
            .fill: .letter("f"),
            .eyedropper: .letter("i"),

            .fit: .letter("0"),
            .smaller: KeySpec(key: "["),
            .larger: KeySpec(key: "]"),
            .undo: .letter("z", .command),
            .redo: .letter("z", [.command, .shift]),
            .open: .letter("o", .command),
            .save: .letter("s", .command),
            .export: .letter("s", [.command, .shift]),
            .clearLayer: .letter("k", .command),

        ]
        switch self {
        case .drawKit:
            break
        case .clipStudio:
            map[.fill] = .letter("g")
        case .photoshop:
            map[.fill] = .letter("g")
        case .procreate:
            map[.fill] = .letter("f")
        }
        return map
    }
}

@MainActor
@Observable
final class ShortcutStore {
    private(set) var preset: ShortcutPreset
    private(set) var bindings: [ShortcutAction: KeySpec]
    /// Bumped on every edit, so the menu bar can be rebuilt with the new equivalents.
    private(set) var revision: UInt64 = 0

    @ObservationIgnored private let defaults: UserDefaults
    private static let storageKey = "drawkit.shortcuts.v1"

    init(defaults: UserDefaults = .standard) {
        self.defaults = defaults
        if let data = defaults.data(forKey: Self.storageKey),
           let stored = try? JSONDecoder().decode(Stored.self, from: data) {
            preset = stored.preset
            var loaded: [ShortcutAction: KeySpec] = [:]
            for (raw, spec) in stored.bindings {
                if let action = ShortcutAction(rawValue: raw) {
                    loaded[action] = spec
                }
            }
            let base = stored.preset.bindings
            for (action, spec) in base where loaded[action] == nil {
                loaded[action] = spec
            }
            bindings = loaded
        } else {
            preset = .drawKit
            bindings = ShortcutPreset.drawKit.bindings
        }
    }

    func selectPreset(_ preset: ShortcutPreset) {
        self.preset = preset
        bindings = preset.bindings
        changed()
    }

    func assign(_ spec: KeySpec, to action: ShortcutAction) {
        var copy = bindings
        copy[action] = spec
        bindings = copy
        changed()
    }

    func setKey(_ text: String, for action: ShortcutAction) {
        guard let character = text.lowercased().first, !character.isWhitespace else {
            return
        }
        var spec = bindings[action] ?? KeySpec(key: "?")
        spec.key = String(character)
        assign(spec, to: action)
    }

    func toggle(_ modifier: ShortcutModifiers, for action: ShortcutAction) {
        var spec = bindings[action] ?? KeySpec(key: "?")
        if spec.modifiers.contains(modifier) {
            spec.modifiers.remove(modifier)
        } else {
            spec.modifiers.insert(modifier)
        }
        assign(spec, to: action)
    }

    func keyString(_ action: ShortcutAction) -> String {
        bindings[action]?.key ?? ""
    }

    func display(_ action: ShortcutAction) -> String {
        bindings[action]?.display ?? "—"
    }

    func keyEquivalent(for action: ShortcutAction) -> KeyEquivalent {
        bindings[action]?.keyEquivalent ?? "?"
    }

    func eventModifiers(for action: ShortcutAction) -> EventModifiers {
        bindings[action]?.modifiers.eventModifiers ?? []
    }

    func modifiers(for action: ShortcutAction) -> ShortcutModifiers {
        bindings[action]?.modifiers ?? []
    }

    /// Which action a canvas key-down means, if any. Menu actions never reach here: the menu owns
    /// their key equivalents, so a command-key binding is not stolen by the canvas.
    func action(for event: NSEvent, scope: ShortcutScope) -> ShortcutAction? {
        let key = event.charactersIgnoringModifiers?.lowercased() ?? ""
        let modifiers = ShortcutModifiers(eventFlags: event.modifierFlags)
        return ShortcutAction.allCases.first { action in
            guard action.scope == scope, let spec = bindings[action] else { return false }
            return spec.key == key && spec.modifiers == modifiers
        }
    }

    var conflictingActions: Set<ShortcutAction> {
        var groups: [String: [ShortcutAction]] = [:]
        for action in ShortcutAction.allCases {
            guard let spec = bindings[action] else { continue }
            groups[spec.display, default: []].append(action)
        }
        var conflicts: Set<ShortcutAction> = []
        for actions in groups.values where actions.count > 1 {
            conflicts.formUnion(actions)
        }
        return conflicts
    }

    private func changed() {
        revision &+= 1
        save()
    }

    private func save() {
        var storedBindings: [String: KeySpec] = [:]
        for (action, spec) in bindings {
            storedBindings[action.rawValue] = spec
        }
        let stored = Stored(preset: preset, bindings: storedBindings)
        if let data = try? JSONEncoder().encode(stored) {
            defaults.set(data, forKey: Self.storageKey)
        }
    }

    private struct Stored: Codable {
        var preset: ShortcutPreset
        var bindings: [String: KeySpec]
    }
}
