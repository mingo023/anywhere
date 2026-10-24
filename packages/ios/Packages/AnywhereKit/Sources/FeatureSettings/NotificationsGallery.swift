import Client
import DesignSystem
import Foundation
import Stores
import SwiftUI

/// The notification settings with every registration state a Mac row shows, and with notifications turned off.
public enum NotificationsGallery {
    @MainActor public static var scenes: [GalleryScene] {
        [("notifications", true), ("notifications-off", false)].flatMap { name, allowed in
            [false, true].map { dark in
                GalleryScene("settings-\(name)\(dark ? "-dark" : "")", dark: dark) {
                    AnyView(Form { NotificationsSection(center: center(allowed: allowed), rows: rows) }.formStyle(.grouped))
                }
            }
        }
    }

    @MainActor static func center(allowed: Bool) -> PushCenter {
        let defaults = UserDefaults(suiteName: "anywhere-gallery-push")!
        defaults.removePersistentDomain(forName: "anywhere-gallery-push")
        let center = PushCenter(defaults: defaults, keys: Keychain(service: "anywhere-gallery-push"))
        center.granted(allowed)
        return center
    }

    @MainActor static var rows: [NotificationsSection.Row] {
        [
            .init(id: "m1", name: "Mingo's MacBook Pro", state: .registered, test: {}),
            .init(id: "m2", name: "Studio", state: .failed("The push relay refused this phone (RateLimited)."), test: {}),
            .init(id: "m3", name: "Old iMac", state: .unsupported, test: {}),
        ]
    }
}
