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
                Section(settings.t(.language)) {
                    Picker(settings.t(.language), selection: $settings.language) {
                        ForEach(IOSLanguage.allCases) { language in
                            Text(language.title).tag(language)
                        }
                    }
                    Text(settings.t(.languageHelp))
                        .font(.footnote)
                        .foregroundStyle(Brand.muted)
                }

                Section {
                    Toggle(settings.t(.hdr), isOn: Binding(
                        get: { studio.hdr },
                        set: { studio.setHdr($0) }
                    ))
                    Text(settings.t(.hdrHelp))
                        .font(.footnote)
                        .foregroundStyle(Brand.muted)
                }

                Section(settings.t(.startup)) {
                    Button(settings.t(.showTour)) {
                        settings.hasCompletedOnboarding = false
                        dismiss()
                    }
                    Text(settings.t(.showTourHelp))
                        .font(.footnote)
                        .foregroundStyle(Brand.muted)
                }

                Section(settings.t(.settingsAbout)) {
                    LabeledContent(settings.t(.version), value: version)
                    Text(settings.t(.aboutBody))
                        .font(.footnote)
                        .foregroundStyle(Brand.muted)
                }
            }
            .navigationTitle(settings.t(.settingsTitle))
            .toolbar {
                ToolbarItem(placement: .confirmationAction) {
                    Button("Done") { dismiss() }
                }
            }
        }
        .preferredColorScheme(.dark)
    }
}
