import Client
import PushSupport
import UIKit
import UserNotifications

final class AppDelegate: NSObject, UIApplicationDelegate, UNUserNotificationCenterDelegate {
    func application(_ application: UIApplication, didFinishLaunchingWithOptions launchOptions: [UIApplication.LaunchOptionsKey: Any]? = nil) -> Bool {
        let center = UNUserNotificationCenter.current()
        center.delegate = self
        center.setNotificationCategories(Self.categories)
        Task {
            if (try? await center.requestAuthorization(options: [.alert, .badge, .sound])) == true {
                application.registerForRemoteNotifications()
            }
        }
        return true
    }

    /// The push store registers this token with the relay, then hands the handle to each Mac.
    func application(_ application: UIApplication, didRegisterForRemoteNotificationsWithDeviceToken deviceToken: Data) {
        let hex = deviceToken.map { String(format: "%02x", $0) }.joined()
        UserDefaults(suiteName: Keychain.appGroup)?.set(hex, forKey: "apns.token")
    }

    func application(_ application: UIApplication, didFailToRegisterForRemoteNotificationsWithError error: any Error) {}

    nonisolated func userNotificationCenter(_ center: UNUserNotificationCenter, willPresent notification: UNNotification) async -> UNNotificationPresentationOptions {
        [.banner, .list, .sound]
    }

    nonisolated func userNotificationCenter(_ center: UNUserNotificationCenter, didReceive response: UNNotificationResponse) async {
        let info = response.notification.request.content.userInfo
        guard let mac = info["mac"] as? String, let agent = info["agent"] as? String else { return }
        let opens = [UNNotificationDefaultActionIdentifier, NotificationAction.open].contains(response.actionIdentifier)
        guard opens, let url = URL(string: "anywhere://mac/\(mac)/agent/\(agent)") else { return }
        await UIApplication.shared.open(url)
    }

    static var categories: Set<UNNotificationCategory> {
        let allow = UNNotificationAction(identifier: NotificationAction.allow, title: "Allow", options: [.authenticationRequired])
        let deny = UNNotificationAction(identifier: NotificationAction.deny, title: "Deny", options: [.destructive, .authenticationRequired])
        let denyNote = UNTextInputNotificationAction(
            identifier: NotificationAction.denyNote, title: "Deny with note…", options: [.destructive, .authenticationRequired],
            textInputButtonTitle: "Deny", textInputPlaceholder: "Tell the agent why")
        let open = UNNotificationAction(identifier: NotificationAction.open, title: "Open", options: [.foreground])
        let reply = UNTextInputNotificationAction(
            identifier: NotificationAction.reply, title: "Reply…", options: [.authenticationRequired], textInputButtonTitle: "Send",
            textInputPlaceholder: "Message")
        return [
            UNNotificationCategory(identifier: NotificationCategory.permission, actions: [allow, deny], intentIdentifiers: []),
            UNNotificationCategory(identifier: NotificationCategory.permissionNote, actions: [allow, deny, denyNote], intentIdentifiers: []),
            UNNotificationCategory(identifier: NotificationCategory.question, actions: [open], intentIdentifiers: []),
            UNNotificationCategory(identifier: NotificationCategory.done, actions: [reply, open], intentIdentifiers: []),
            UNNotificationCategory(identifier: NotificationCategory.failed, actions: [reply, open], intentIdentifiers: []),
            UNNotificationCategory(identifier: NotificationCategory.automation, actions: [open], intentIdentifiers: []),
        ]
    }
}
