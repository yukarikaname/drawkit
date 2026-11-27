import SwiftUI

@main
struct DrawKitApp: App {
    @State private var studio = Studio()
    @State private var settings = AppSettings()

    init() {
        NSLog("DRAWKIT_IOS_LAUNCH bench=%d", CanvasBench.requested ? 1 : 0)
    }

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
