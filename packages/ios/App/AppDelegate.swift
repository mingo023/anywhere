import Client
import Domain
import LiveActivityUI
import PushSupport
import Stores
import UIKit
import UserNotifications
import Wire

@MainActor
final class AppDelegate: NSObject, UIApplicationDelegate, UNUserNotificationCenterDelegate {
    /// Made here rather than in the App so notification actions and Live Activity buttons reach it when iOS launches the app in the background.
    let app = AppModel.restore(from: CredentialsStore(keychain: Keychain(service: CredentialsStore.service, accessGroup: Keychain.appGroup)))

    func application(_ application: UIApplication, didFinishLaunchingWithOptions launchOptions: [UIApplication.LaunchOptionsKey: Any]? = nil) -> Bool {
        let center = UNUserNotificationCenter.current()
        center.delegate = self
        center.setNotificationCategories(Self.categories)
        LiveActivities.install(PushCenter.shared)
        Answering.handler = { [app] mac, request, decision in
            try await app.deliver(.permissionResolve(.init(requestId: request, decision: Decision(rawValue: decision))), mac: mac)
        }
        #if DEBUG || BENCH
            if BenchRun.scenario != nil { return true }
            if LiveActivityCheck.requested {
                LiveActivityCheck.run()
                return true
            }
        #endif
        Task {
            let granted = (try? await center.requestAuthorization(options: [.alert, .badge, .sound])) ?? false
            PushCenter.shared.granted(granted)
            if granted { application.registerForRemoteNotifications() }
        }
        return true
    }

    func application(_ application: UIApplication, didRegisterForRemoteNotificationsWithDeviceToken deviceToken: Data) {
        PushCenter.shared.received(alertToken: deviceToken)
    }

    func application(_ application: UIApplication, didFailToRegisterForRemoteNotificationsWithError error: any Error) {}

    /// pocketd holds alerts while the app is connected, so one arriving in front is for another Mac or a session not on screen.
    nonisolated func userNotificationCenter(_ center: UNUserNotificationCenter, willPresent notification: UNNotification) async -> UNNotificationPresentationOptions {
        let agent = notification.request.content.userInfo["agent"] as? String
        let visible = await PushCenter.shared.visibleAgent
        return agent != nil && agent == visible ? [] : [.banner, .list, .sound]
    }

    nonisolated func userNotificationCenter(_ center: UNUserNotificationCenter, didReceive response: UNNotificationResponse) async {
        var info: [String: String] = [:]
        for case let (key as String, value as String) in response.notification.request.content.userInfo { info[key] = value }
        let text = (response as? UNTextInputNotificationResponse)?.userText
        await handle(action: response.actionIdentifier, info: info, text: text)
    }

    private func handle(action: String, info: [String: String], text: String?) async {
        guard let mac = info["mac"] else { return }
        do {
            switch NotificationReply.of(action: action, userInfo: info, text: text) {
            case .resolve(let request, let decision, let message):
                try await app.deliver(.permissionResolve(.init(requestId: request, decision: Decision(rawValue: decision), message: message)), mac: mac)
            case .prompt(let agent, let text):
                try await app.deliver(.agentPrompt(.init(agentId: agent, text: text)), mac: mac)
            case .open:
                if let link = DeepLink.notification(info) { await UIApplication.shared.open(link.url) }
            case .none:
                break
            }
        } catch {
            let content = UNMutableNotificationContent()
            content.title = "Not sent"
            content.body = PushStore.describe(error)
            content.threadIdentifier = info["agent"] ?? mac
            try? await UNUserNotificationCenter.current().add(UNNotificationRequest(identifier: UUID().uuidString, content: content, trigger: nil))
        }
    }

    static var categories: Set<UNNotificationCategory> {
        Set(NotificationCategory.actions.map { category in
            UNNotificationCategory(identifier: category.id, actions: category.actions.map(action), intentIdentifiers: [], options: [])
        })
    }

    static func action(_ spec: ActionSpec) -> UNNotificationAction {
        let options: UNNotificationActionOptions =
            switch spec.style {
            case .unlocked: [.authenticationRequired]
            case .destructive: [.destructive, .authenticationRequired]
            case .foreground: [.foreground]
            }
        guard let input = spec.input else { return UNNotificationAction(identifier: spec.id, title: spec.title, options: options) }
        return UNTextInputNotificationAction(
            identifier: spec.id, title: spec.title, options: options, textInputButtonTitle: input.button, textInputPlaceholder: input.placeholder)
    }
}
