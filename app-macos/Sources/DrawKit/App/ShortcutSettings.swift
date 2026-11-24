// The shortcut editor. A preset fills the table; every row can be changed afterwards and is saved
// immediately. Conflicts are marked, not blocked: the painter may be mid-reorganisation.

import SwiftUI

struct ShortcutSettingsView: View {
    let store: ShortcutStore
    @Environment(AppSettings.self) private var settings

    var body: some View {
        Form {
            Section {
                Picker(settings.t(.shortcutsPreset), selection: Binding(
                    get: { store.preset },
                    set: { store.selectPreset($0) }
                )) {
                    ForEach(ShortcutPreset.allCases) { preset in
                        Text(preset.title).tag(preset)
                    }
                }
                Text(settings.t(.shortcutsPresetHelp))
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }

            Section(settings.t(.shortcutsCanvas)) {
                ForEach(ShortcutAction.canvasActions) { action in
                    ShortcutRow(store: store, action: action)
                }
            }

            Section(settings.t(.shortcutsApplication)) {
                ForEach(ShortcutAction.applicationActions) { action in
                    ShortcutRow(store: store, action: action)
                }
            }

            Section {
                Button("\(settings.t(.shortcutsReset)) \(store.preset.title)") {
                    store.selectPreset(store.preset)
                }
                Text(settings.t(.shortcutsHelp))
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
            Text(settings.shortcutTitle(action))
                .frame(width: 150, alignment: .leading)

            if store.conflictingActions.contains(action) {
                Image(systemName: "exclamationmark.triangle.fill")
                    .foregroundStyle(.orange)
                    .help(settings.t(.shortcutsConflict))
            }

            Spacer()

            TextField("", text: Binding(
                get: { store.keyString(action) },
                set: { store.setKey($0, for: action) }
            ))
            .frame(width: 38)
            .textFieldStyle(.roundedBorder)
            .multilineTextAlignment(.center)
            .help(settings.t(.shortcutsKey))

            ForEach(Array(ShortcutModifiers.choices.enumerated()), id: \.offset) { _, choice in
                Button(choice.label) {
                    store.toggle(choice.modifier, for: action)
                }
                .buttonStyle(.bordered)
                .controlSize(.mini)
                .tint(store.modifiers(for: action).contains(choice.modifier)
                      ? Color.accentColor
                      : Color.secondary)
                .help(choice.modifier == .command ? settings.t(.shortcutsCommand) :
                      choice.modifier == .shift ? settings.t(.shortcutsShift) :
                      choice.modifier == .option ? settings.t(.shortcutsOption) : settings.t(.shortcutsControl))
            }
        }
    }
}
