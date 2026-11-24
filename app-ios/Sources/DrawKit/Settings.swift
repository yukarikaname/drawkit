// The iPad's settings. The copy comes from `shared/strings.json` through `dk_text`, the same table
// the macOS shell reads; only the language choice and the onboarding flag are stored here.

import Foundation
import Observation

enum IOSLanguage: String, CaseIterable, Identifiable {
    case system
    case english
    case simplifiedChinese
    case japanese

    var id: Self { self }

    var title: String {
        switch self {
        case .system: "System"
        case .english: "English"
        case .simplifiedChinese: "简体中文"
        case .japanese: "日本語"
        }
    }

    var code: String {
        switch self {
        case .system, .english: "en"
        case .simplifiedChinese: "zh-Hans"
        case .japanese: "ja"
        }
    }
}

enum IOSKey: String {
    case settingsTitle, settingsGeneral, settingsAbout
    case language, languageHelp, hdr, hdrHelp, startup, showTour, showTourHelp, version, aboutBody
    case onboardingSkip, onboardingBack, onboardingNext, onboardingStart
    case onboarding1Title, onboarding1Body
    case onboarding2Title, onboarding2Body
    case onboarding3Title, onboarding3Body
    case onboarding4Title, onboarding4Body
}

@Observable
final class AppSettings {
    var language: IOSLanguage {
        didSet { defaults.set(language.rawValue, forKey: Keys.language) }
    }

    var hasCompletedOnboarding: Bool {
        didSet { defaults.set(hasCompletedOnboarding, forKey: Keys.onboarding) }
    }

    private let defaults = UserDefaults.standard

    init() {
        let raw = UserDefaults.standard.string(forKey: Keys.language)
            ?? IOSLanguage.system.rawValue
        language = IOSLanguage(rawValue: raw) ?? .system
        hasCompletedOnboarding = UserDefaults.standard.bool(forKey: Keys.onboarding)
    }

    var effectiveLanguage: IOSLanguage {
        guard language == .system else { return language }
        let preferred = Locale.preferredLanguages.first ?? "en"
        if preferred.hasPrefix("zh") { return .simplifiedChinese }
        if preferred.hasPrefix("ja") { return .japanese }
        return .english
    }

    func t(_ key: IOSKey) -> String {
        Self.text(key.rawValue, language: effectiveLanguage.code)
    }

    private static func text(_ key: String, language: String) -> String {
        key.withCString { keyPointer in
            language.withCString { languagePointer in
                let length = dk_text(keyPointer, languagePointer, nil, 0)
                guard length > 0 else { return key }
                var buffer = [CChar](repeating: 0, count: Int(length) + 1)
                _ = buffer.withUnsafeMutableBufferPointer {
                    dk_text(keyPointer, languagePointer, $0.baseAddress, $0.count)
                }
                let bytes = buffer.prefix(Int(length)).map { UInt8(bitPattern: $0) }
                return String(decoding: bytes, as: UTF8.self)
            }
        }
    }

    private enum Keys {
        static let language = "dev.drawkit.language"
        static let onboarding = "dev.drawkit.onboardingComplete"
    }
}
