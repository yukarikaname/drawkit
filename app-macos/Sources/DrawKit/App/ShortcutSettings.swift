// The shortcut editor. A preset fills the table; every row can be changed afterwards and is saved
// immediately. Conflicts are marked, not blocked: the painter may be mid-reorganisation.

import SwiftUI

struct ShortcutSettingsView: View {
    let store: ShortcutStore
    @Environment(AppSettings.self) private var settings

    var body: some View {
        Form {
            Section {
                Picker("Preset", selection: Binding(
                    get: { store.preset },
                    set: { store.selectPreset($0) }
                )) {
                    ForEach(ShortcutPreset.allCases) { preset in
                        Text(preset.title).tag(preset)
                    }
                }
                Text("Presets map the actions this app has. Tools the source app has and we do not are left out, not faked.")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }

            Section("Canvas") {
                ForEach(ShortcutAction.canvasActions) { action in
                    ShortcutRow(store: store, action: action)
                }
            }

            Section("Application") {
                ForEach(ShortcutAction.applicationActions) { action in
                    ShortcutRow(store: store, action: action)
                }
            }

            Section {
                Button {
                    store.selectPreset(store.preset)
                } label: {
                    Text("Reset") + Text(verbatim: " ") + Text(store.preset.title)
                }
                Text("Type a key and choose its modifiers. A warning means two actions share the same combination.")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
        }
        .formStyle(.grouped)
    }
}

struct ShortcutRow: View {
    let store: ShortcutStore
    let action: ShortcutAction
    @Environment(AppSettings.self) private var settings

    var body: some View {
        HStack(spacing: 8) {
            Text(action.title)
                .frame(width: 150, alignment: .leading)

            if store.conflictingActions.contains(action) {
                Image(systemName: "exclamationmark.triangle.fill")
                    .foregroundStyle(.orange)
                    .help("Another action uses this shortcut")
            }

            Spacer()

            TextField("", text: Binding(
                get: { store.keyString(action) },
                set: { store.setKey($0, for: action) }
            ))
            .frame(width: 38)
            .textFieldStyle(.roundedBorder)
            .multilineTextAlignment(.center)
            .help("Single key for this action")

            ForEach(Array(ShortcutModifiers.choices.enumerated()), id: \.offset) { _, choice in
                Button(choice.label) {
                    store.toggle(choice.modifier, for: action)
                }
                .buttonStyle(.bordered)
                .controlSize(.mini)
                .tint(store.modifiers(for: action).contains(choice.modifier)
                      ? Color.accentColor
                      : Color.secondary)
                .help(choice.modifier == .command ? "Command" :
                      choice.modifier == .shift ? "Shift" :
                      choice.modifier == .option ? "Option" : "Control")
            }
        }
    }
}
