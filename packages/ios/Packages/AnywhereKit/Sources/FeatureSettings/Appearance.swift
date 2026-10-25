import SwiftUI

/// Light, dark, or following the system; kept in `@AppStorage(Appearance.key)`.
public enum Appearance: String, CaseIterable, Identifiable, Sendable {
    case system
    case light
    case dark

    public static let key = "appearance"

    public var id: String { rawValue }

    public var label: String {
        switch self {
        case .system: "System"
        case .light: "Light"
        case .dark: "Dark"
        }
    }

    /// What `preferredColorScheme` takes; nil follows the system.
    public var colorScheme: ColorScheme? {
        switch self {
        case .system: nil
        case .light: .light
        case .dark: .dark
        }
    }
}
