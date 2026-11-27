import SwiftUI

@main
struct DrawKitApp: App {
    @State private var studio = Studio()
    @State private var settings = AppSettings()

    var body: some Scene {
        WindowGroup {
            ContentView(studio: studio)
                .environment(settings)
                .drawKitLayoutDirection()
        }
        .commands {
            DrawKitCommands(studio: studio, settings: settings)
        }
    }
}
