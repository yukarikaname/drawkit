// The iPad settings sheet: language, replaying the tour, and what this build is. The copy is shared
// with the macOS window through `shared/strings.json`.

import SwiftUI

struct SettingsSheet: View {
    @Bindable var settings: AppSettings
    let studio: Studio
    @Environment(\.dismiss) private var dismiss

    private var version: String {
        Bundle.main.infoDictionary?["CFBundleShortVersionString"] as? String ?? "0.1"
    }

    var body: some View {
        NavigationStack {
            Form {
                Section {
                    Toggle("HDR · extended range", isOn: Binding(
                        get: { studio.hdr },
                        set: { studio.setHdr($0) }
                    ))
                    Text("Keeps highlights above paper white; export EXR to keep them.")
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
                    Text("One Rust core, the same layer tree and the same strokes on every screen. The iPad canvas composites on the CPU; HDR and the settings window came first on macOS.")
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
