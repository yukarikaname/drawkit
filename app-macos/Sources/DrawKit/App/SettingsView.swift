// Settings is a tabbed window: general preferences, the shortcut table, and the small about card.

import SwiftUI

struct SettingsView: View {
    let shortcuts: ShortcutStore
    @Environment(AppSettings.self) private var settings

    var body: some View {
        TabView {
            GeneralSettingsView()
                .tabItem { Label(settings.t(.settingsGeneral), systemImage: "gearshape") }
            ShortcutSettingsView(store: shortcuts)
                .tabItem { Label(settings.t(.settingsShortcuts), systemImage: "keyboard") }
            AboutSettingsView()
                .tabItem { Label(settings.t(.settingsAbout), systemImage: "info.circle") }
        }
        .frame(width: 620, height: 660)
    }
}

private struct GeneralSettingsView: View {
    @Environment(AppSettings.self) private var settings
    @AppStorage("drawkit.experimental.historyBranches")
    private var historyBranches = false

    var body: some View {
        Form {
            Section(settings.t(.language)) {
                Picker(settings.t(.language), selection: Binding(
                    get: { settings.language },
                    set: { settings.language = $0 }
                )) {
                    ForEach(AppLanguage.allCases) { language in
                        Text(language.title).tag(language)
                    }
                }
                .pickerStyle(.menu)
                Text(settings.t(.languageHelp))
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }

            Section("Experimental") {
                Toggle("Branching history", isOn: $historyBranches)
                Text("Undoing and drawing again keeps the abandoned path as a dormant branch. "
                     + "Branches live in memory only and are not saved in the project.")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }

            Section(settings.t(.onboardingSensitivity)) {
                HStack {
                    PenSlider(value: Binding(
                        get: { settings.pressureCurve },
                        set: { settings.pressureCurve = min(max($0, 0.5), 2.0) }
                    ), range: 0.5...2.0, width: 190)
                    Text(String(format: "%.2f", settings.pressureCurve))
                        .font(.system(size: 12).monospacedDigit())
                        .foregroundStyle(.secondary)
                        .frame(width: 34, alignment: .trailing)
                }
                Text(settings.t(.pressureHelp))
                    .font(.caption)
                    .foregroundStyle(.secondary)
                HStack {
                    Text(String(format: "Raw range %.2f – %.2f",
                                settings.pressureLow, settings.pressureHigh))
                        .font(.caption.monospacedDigit())
                        .foregroundStyle(.secondary)
                    Spacer()
                    Button("Reset") {
                        settings.pressureLow = 0
                        settings.pressureHigh = 1
                    }
                    .controlSize(.small)
                }
            }

            Section("Autosave") {
                Picker("Recovery copy", selection: Binding(
                    get: { settings.autosaveInterval },
                    set: { settings.autosaveInterval = $0 }
                )) {
                    Text("Off").tag(0.0)
                    Text("Every 15 seconds").tag(15.0)
                    Text("Every 30 seconds").tag(30.0)
                    Text("Every minute").tag(60.0)
                    Text("Every 2 minutes").tag(120.0)
                }
                .pickerStyle(.menu)
                Text("A crash or a quit without saving leaves the last copy for recovery.")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }

            Section("Tablet") {
                Picker("Pen button 1", selection: Binding(
                    get: { settings.tabletPrimary },
                    set: { settings.tabletPrimary = $0 }
                )) {
                    ForEach(TabletButtonAction.allCases) { action in
                        Text(action.title).tag(action)
                    }
                }
                .pickerStyle(.menu)
                Picker("Pen button 2", selection: Binding(
                    get: { settings.tabletSecondary },
                    set: { settings.tabletSecondary = $0 }
                )) {
                    ForEach(TabletButtonAction.allCases) { action in
                        Text(action.title).tag(action)
                    }
                }
                .pickerStyle(.menu)
                Text("Set the two pen buttons to Right Click and Middle Click in the tablet driver, then choose what each click does here. Force Touch does not need this; it opens the quick menu directly.")
                    .font(.caption)
                    .foregroundStyle(.secondary)
                Text("Turn on Force Proportions (强制等比例) in the tablet's mapping settings and match its area to the screen aspect ratio. Otherwise circles draw as ellipses.")
                    .font(.caption)
                    .foregroundStyle(.secondary)
                Button("Show the mapping reminder again") {
                    settings.hasSeenTabletTip = false
                }
                .controlSize(.small)
            }

            Section(settings.t(.startup)) {
                Button(settings.t(.showTour)) {
                    settings.hasCompletedOnboarding = false
                }
                Text(settings.t(.showTourHelp))
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
        }
        .formStyle(.grouped)
    }
}

private struct AboutSettingsView: View {
    @Environment(AppSettings.self) private var settings

    private var version: String {
        Bundle.main.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String ?? "0.1.0"
    }

    var body: some View {
        VStack(spacing: 14) {
            BrandMark(size: 64, line: Brand.paper)
            Text("DrawKit")
                .font(.system(size: 28, weight: .bold, design: .rounded))
                .foregroundStyle(Brand.paper)
            Text(settings.t(.version) + " " + version)
                .font(.system(size: 12))
                .foregroundStyle(Brand.muted)
            Text("A practical painting kit. Same core, same pixels, every screen.")
                .font(.system(size: 12))
                .foregroundStyle(Brand.paper.opacity(0.65))
                .multilineTextAlignment(.center)
                .frame(maxWidth: 360)
            Spacer()
            Text("MIT licensed · Rust core · wgpu")
                .font(.caption2)
                .foregroundStyle(Brand.muted)
        }
        .padding(36)
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(Brand.ink)
        .preferredColorScheme(.dark)
    }
}
