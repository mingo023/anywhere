import Client
import DesignSystem
import Foundation
import Stores
import SwiftUI
import Wire

/// The notification settings as the design draws them: alerts refused in iOS Settings, with some kinds on.
public enum NotificationsGallery {
    @MainActor public static var scenes: [GalleryScene] {
        [false, true].map { dark in
            GalleryScene("36-settings-notifications-\(dark ? "dark" : "light")", dark: dark) {
                AnyView(NavigationStack { NotificationsView(center: center()) })
            }
        }
    }

    @MainActor static func center() -> PushCenter {
        let defaults = UserDefaults(suiteName: "anywhere-gallery-push")!
        defaults.removePersistentDomain(forName: "anywhere-gallery-push")
        let center = PushCenter(defaults: defaults, keys: Keychain(service: "anywhere-gallery-push"))
        center.granted(false)
        center.set(PushPrefs(needsYou: true, done: false, failed: true, automations: false, details: true))
        return center
    }
}
