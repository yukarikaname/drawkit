// The iPad settings sheet: language, replaying the tour, and what this build is. The copy is shared
// with the macOS window through `shared/strings.json`.

import SwiftUI
import UIKit

struct SettingsSheet: View {
    @Bindable var settings: AppSettings
    let studio: Studio
    @Environment(\.dismiss) private var dismiss

    private var version: String {
        Bundle.main.infoDictionary?["CFBundleShortVersionString"] as? String ?? "0.1"
    }

    /// What a double tap will do, as the person chose it in Settings > Apple Pencil. Showing the
    /// system value rather than a fixed string makes the sheet honest when the choice changed.
    private var doubleTapTitle: String {
        title(for: UIPencilInteraction.preferredTapAction)
    }

    /// Squeeze is DrawKit's eraser switch unless the person turned it off or gave it to a system
    /// shortcut; in those two cases the app deliberately stays out of the way.
    private var squeezeTitle: String {
        switch UIPencilInteraction.preferredSqueezeAction {
        case .ignore: return "Off"
        case .runSystemShortcut: return "Run Shortcut"
        default: return "Toggle Eraser"
        }
    }

    private func title(for action: UIPencilPreferredAction) -> String {
        switch action {
        case .ignore: return "Off"
        case .switchEraser: return "Switch to Eraser"
        case .switchPrevious: return "Switch to Previous Tool"
        case .showColorPalette: return "Show Colour Palette"
        case .showInkAttributes: return "Show Ink Attributes"
        case .showContextualPalette: return "Show Contextual Palette"
        case .runSystemShortcut: return "Run Shortcut"
        @unknown default: return "System Action"
        }
    }

    var body: some View {
        NavigationStack {
            Form {
                Section {
                    Toggle(
                        "HDR · extended range",
                        isOn: Binding(
                            get: { studio.hdr },
                            set: { studio.setHdr($0) }
                        ))
                    Text("Keeps highlights above paper white; export EXR to keep them.")
                        .font(.footnote)
                        .foregroundStyle(Brand.muted)
                }

                Section("Apple Pencil") {
                    LabeledContent("Double tap", value: doubleTapTitle)
                    LabeledContent("Squeeze", value: squeezeTitle)
                    Text(
                        "Double tap follows Settings > Apple Pencil. Squeeze toggles the eraser, "
                            + "unless it is turned off or assigned a Shortcut there."
                    )
                    .font(.footnote)
                    .foregroundStyle(Brand.muted)
                }

                Section("Startup") {
                    Button("Show the welcome tour") {
                        settings.hasCompletedOnboarding = false
                        dismiss()
                    }
                    Text("Open the four short pages again, over the painting or welcome screen.")
                        .font(.footnote)
                        .foregroundStyle(Brand.muted)
                }

                Section("About") {
                    LabeledContent("Version", value: version)
                    Text(
                        "One Rust core, the same layer tree and the same strokes on every screen. The iPad canvas composites on the CPU; HDR and the settings window came first on macOS."
                    )
                    .font(.footnote)
                    .foregroundStyle(Brand.muted)
                }
            }
            .navigationTitle("Settings")
            .toolbar {
                ToolbarItem(placement: .confirmationAction) {
                    Button("Done") { dismiss() }
                }
            }
        }
        .preferredColorScheme(.dark)
    }
}
