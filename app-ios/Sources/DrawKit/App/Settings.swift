// The iPad's settings. The language and every label come from the String Catalog in the bundle, so
// Settings > DrawKit > Language owns the choice; only the onboarding flag is stored here.

import Foundation
import Observation

@Observable
final class AppSettings {
    var hasCompletedOnboarding: Bool {
        didSet { defaults.set(hasCompletedOnboarding, forKey: Keys.onboarding) }
    }

    private let defaults = UserDefaults.standard

    init() {
        hasCompletedOnboarding = UserDefaults.standard.bool(forKey: Keys.onboarding)
    }

    private enum Keys {
        static let onboarding = "dev.drawkit.onboardingComplete"
    }
}
