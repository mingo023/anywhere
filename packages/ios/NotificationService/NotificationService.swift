import Client
import CryptoKit
import PushSupport
import UserNotifications

/// Opens an alert's sealed `e` and writes what it says into the banner; on any failure the banner stays as sent.
final class NotificationService: UNNotificationServiceExtension {
    private var handler: ((UNNotificationContent) -> Void)?
    private var best: UNMutableNotificationContent?

    override func didReceive(_ request: UNNotificationRequest, withContentHandler contentHandler: @escaping (UNNotificationContent) -> Void) {
        handler = contentHandler
        guard let content = request.content.mutableCopy() as? UNMutableNotificationContent else { return contentHandler(request.content) }
        best = content
        if let e = content.userInfo["e"] as? String, let notice = NoticeBox.open(e, keys: Self.keys()) {
            content.title = notice.title
            content.subtitle = notice.place
            content.body = notice.line
            content.targetContentIdentifier = "agent:\(notice.agent)"
            var info: [String: Any] = ["mac": notice.mac, "agent": notice.agent]
            if let request = notice.request { info["request"] = request }
            content.userInfo = info
            if let defaults = UserDefaults(suiteName: Keychain.appGroup) {
                content.badge = NSNumber(value: BadgeLedger(defaults: defaults).record(mac: notice.mac, count: notice.badge))
            }
        }
        contentHandler(content)
    }

    override func serviceExtensionTimeWillExpire() {
        if let best { handler?(best) }
    }

    /// Each paired Mac's sealing key, saved by the app when it registered for pushes.
    static func keys() -> [String: SymmetricKey] {
        let keychain = Keychain(service: NoticeBox.keychainService, accessGroup: Keychain.appGroup)
        var keys: [String: SymmetricKey] = [:]
        for mac in (try? keychain.accounts()) ?? [] {
            if let data = try? keychain.data(for: mac) { keys[mac] = SymmetricKey(data: data) }
        }
        return keys
    }
}
