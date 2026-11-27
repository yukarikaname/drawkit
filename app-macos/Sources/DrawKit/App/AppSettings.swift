// App-level settings. The strings live in `shared/strings.json`; the Apple targets read them from
// the generated String Catalog, the other shells through `dk_text`. This file keeps the language
// choice, the onboarding flag and the stored key names, not translations.

import Foundation
import Observation

/// What the two buttons on a pen or tablet do when the driver sends a right or middle click.
enum TabletButtonAction: String, CaseIterable, Identifiable {
    case quickMenu
    case eyedropper
    case undo
    case redo
    case brush
    case eraser
    case layerUp
    case layerDown
    case pan
    case none

    var id: Self { self }

    var title: LocalizedStringResource {
        switch self {
        case .quickMenu: "Quick menu"
        case .eyedropper: "Eyedropper"
        case .undo: "Undo"
        case .redo: "Redo"
        case .brush: "Brush"
        case .eraser: "Eraser"
        case .layerUp: "Layer above"
        case .layerDown: "Layer below"
        case .pan: "Pan canvas"
        case .none: "Nothing"
        }
    }
}

enum AppLanguage: String, CaseIterable, Identifiable {
    case system
    case english
    case simplifiedChinese
    case traditionalChinese
    case japanese
    case korean
    case french

    var id: Self { self }

    var title: String {
        switch self {
        case .system: "System"
        case .english: "English"
        case .simplifiedChinese: "简体中文"
        case .traditionalChinese: "繁體中文"
        case .japanese: "日本語"
        case .korean: "한국어"
        case .french: "Français"
        }
    }

    /// The code used in `shared/strings.json`. System resolves before this is read.
    var code: String {
        switch self {
        case .system: "en"
        case .english: "en"
        case .simplifiedChinese: "zh-Hans"
        case .traditionalChinese: "zh-Hant"
        case .japanese: "ja"
        case .korean: "ko"
        case .french: "fr"
        }
    }
}

@MainActor
@Observable
final class AppSettings {
    var language: AppLanguage {
        didSet {
            defaults.set(language.rawValue, forKey: Keys.language)
            // The catalog is read from the bundle, and a bundle picks its language at launch.
            // Writing the per-app preference Apple uses means the next launch is already right.
            switch language {
            case .system: defaults.removeObject(forKey: "AppleLanguages")
            default: defaults.set([language.code], forKey: "AppleLanguages")
            }
        }
    }

    var hasCompletedOnboarding: Bool {
        didSet { defaults.set(hasCompletedOnboarding, forKey: Keys.onboarding) }
    }

    /// Gamma on the tablet pressure: 1 is linear, above 1 needs a firmer touch, below 1 is lighter.
    var pressureCurve: Double {
        didSet { defaults.set(pressureCurve, forKey: Keys.pressureCurve) }
    }

    /// The raw pressure range the tablet actually reports. Mapping it to 0...1 before the gamma is
    /// what makes a light pen usable on a device whose sensor starts at 0.2 or stops at 0.8.
    var pressureLow: Double {
        didSet { defaults.set(pressureLow, forKey: Keys.pressureLow) }
    }

    var pressureHigh: Double {
        didSet { defaults.set(pressureHigh, forKey: Keys.pressureHigh) }
    }

    /// Seconds between recovery copies; zero turns autosave off.
    var autosaveInterval: Double {
        didSet { defaults.set(autosaveInterval, forKey: Keys.autosave) }
    }

    /// The pen's lower/primary barrel button and upper/secondary one. The driver sends these as a
    /// right and a middle click.
    var tabletPrimary: TabletButtonAction {
        didSet { defaults.set(tabletPrimary.rawValue, forKey: Keys.tabletPrimary) }
    }

    var tabletSecondary: TabletButtonAction {
        didSet { defaults.set(tabletSecondary.rawValue, forKey: Keys.tabletSecondary) }
    }

    /// One-time mapping reminder; a person can bring the tour back from Settings.
    var hasSeenTabletTip: Bool {
        didSet { defaults.set(hasSeenTabletTip, forKey: Keys.tabletTip) }
    }

    private let defaults = UserDefaults.standard

    init() {
        let raw = UserDefaults.standard.string(forKey: Keys.language) ?? AppLanguage.system.rawValue
        language = AppLanguage(rawValue: raw) ?? .system
        hasCompletedOnboarding = UserDefaults.standard.bool(forKey: Keys.onboarding)
        let curve = UserDefaults.standard.double(forKey: Keys.pressureCurve)
        pressureCurve = curve == 0 ? 1.0 : min(max(curve, 0.5), 2.0)
        autosaveInterval = UserDefaults.standard.object(forKey: Keys.autosave) as? Double ?? 60
        pressureLow = UserDefaults.standard.object(forKey: Keys.pressureLow) as? Double ?? 0
        tabletPrimary =
            TabletButtonAction(
                rawValue: UserDefaults.standard.string(forKey: Keys.tabletPrimary) ?? "")
            ?? .quickMenu
        tabletSecondary =
            TabletButtonAction(
                rawValue: UserDefaults.standard.string(forKey: Keys.tabletSecondary) ?? "") ?? .pan
        hasSeenTabletTip = UserDefaults.standard.bool(forKey: Keys.tabletTip)
        pressureHigh = UserDefaults.standard.object(forKey: Keys.pressureHigh) as? Double ?? 1
        if pressureHigh - pressureLow < 0.05 {
            pressureLow = 0
            pressureHigh = 1
        }
    }

    private enum Keys {
        static let language = "dev.drawkit.language"
        static let onboarding = "dev.drawkit.onboardingComplete"
        static let pressureCurve = "dev.drawkit.pressureCurve"
        static let autosave = "dev.drawkit.autosaveInterval"
        static let pressureLow = "dev.drawkit.pressureLow"
        static let pressureHigh = "dev.drawkit.pressureHigh"
        static let tabletPrimary = "dev.drawkit.tabletPrimary"
        static let tabletSecondary = "dev.drawkit.tabletSecondary"
        static let tabletTip = "dev.drawkit.tabletTipSeen"
    }
}
