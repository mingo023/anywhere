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
            let text = BannerText(notice)
            content.title = text.title
            content.subtitle = text.subtitle
            content.body = text.body
            content.targetContentIdentifier = notice.targetContentIdentifier
            content.relevanceScore = notice.relevance
            content.userInfo = notice.userInfo
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

/// What a banner says, as the design's Needs you banner reads: what it is, what it asks, and where. pocketd's `line` is the
/// ask, a reply's first paragraph, or the kind's own word when there is nothing else; then the session's title says more.
struct BannerText: Equatable {
    var title: String
    var subtitle: String
    var body: String

    init(_ notice: Notice) {
        let kind = Self.words[notice.kind]
        title = kind ?? notice.title
        subtitle = kind == nil ? notice.place : notice.line == kind ? notice.title : notice.line
        body = kind == nil ? notice.line : notice.place
    }

    private static let words = ["needsYou": "Needs you", "done": "Done", "failed": "Failed"]
}
