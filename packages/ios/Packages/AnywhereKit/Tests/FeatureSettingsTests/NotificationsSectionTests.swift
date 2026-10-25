import Stores
import Testing
@testable import FeatureSettings

@Suite("Notifications settings")
struct NotificationsSectionTests {
    @Test("each Mac row says where its registration stands")
    func status() {
        #expect(NotificationsSection.status(.registered, permission: .granted) == "Notifies this phone")
        #expect(NotificationsSection.status(.waiting, permission: .denied) == "Notifications are off")
        #expect(NotificationsSection.status(.waiting, permission: .undetermined) == "Not registered yet")
        #expect(NotificationsSection.status(.failed("Nope."), permission: .granted) == "Nope.")
        #expect(NotificationsSection.failed(.failed("x")) && !NotificationsSection.failed(.registered))
    }

    @MainActor
    @Test("the gallery shows the settings on and off, in light and dark")
    func gallery() {
        #expect(NotificationsGallery.scenes.map(\.name) == [
            "settings-notifications", "settings-notifications-dark", "settings-notifications-off", "settings-notifications-off-dark",
        ])
    }
}
