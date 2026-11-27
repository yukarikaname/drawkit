// The entry point. The window is SwiftUI; the pixels are the Rust core's.
//
// The document model lives at app level so the menu bar and the window talk to the same object.
// The tool shortcuts stay on the canvas; the menu owns the file and edit shortcuts.

import SwiftUI

@main
struct DrawKitApp: App {
    @NSApplicationDelegateAdaptor(AppDelegate.self) private var appDelegate
    @State private var model = PaintModel()
    @State private var figure = FigureModel()
    @State private var shortcuts = ShortcutStore()
    @State private var workspace = Workspace()
    @State private var settings = AppSettings()

    var body: some Scene {
        WindowGroup("DrawKit") {
            ContentView(model: model, figure: figure, shortcuts: shortcuts, workspace: workspace)
                .environment(settings)
                .drawKitLayoutDirection()
                .onAppear { appDelegate.attach(model: model) }
        }
        // A docked rail and a 288 pt inspector do not fit below this; without the content minimum
        // AppKit lets the window shrink and clips the inspector outside the frame.
        .windowResizability(.contentMinSize)
        .defaultSize(width: 1440, height: 900)
        .commands {
            AppCommands(model: model, shortcuts: shortcuts, workspace: workspace,
                        settings: settings)
        }
        Settings {
            SettingsView(shortcuts: shortcuts)
                .environment(settings)
        }
    }
}
