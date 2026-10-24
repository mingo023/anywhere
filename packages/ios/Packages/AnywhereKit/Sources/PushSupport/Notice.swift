import CryptoKit
import Foundation

/// What an alert's sealed `e` opens to.
public struct Notice: Codable, Hashable, Sendable {
    /// The Mac's `hello.ok` serverId.
    public var mac: String
    public var agent: String
    /// needsYou, done, failed, automation or test.
    public var kind: String
    public var title: String
    /// "project · worktree".
    public var place: String
    public var line: String
    public var request: String?
    /// Sessions wanting a look on this Mac.
    public var badge: Int

    public init(mac: String, agent: String, kind: String, title: String, place: String, line: String, request: String? = nil, badge: Int) {
        self.mac = mac
        self.agent = agent
        self.kind = kind
        self.title = title
        self.place = place
        self.line = line
        self.request = request
        self.badge = badge
    }
}

/// AES-256-GCM as pocketd seals alerts: base64 of nonce(12) ‖ ciphertext ‖ tag(16), the Mac's serverId as associated data.
public enum NoticeBox {
    /// The keychain service holding each Mac's key, one item per Mac serverId.
    public static let keychainService = "dev.mingo.anywhere.push"

    public static func seal(_ notice: Notice, key: SymmetricKey) throws -> String {
        let box = try AES.GCM.seal(JSONEncoder().encode(notice), using: key, authenticating: Data(notice.mac.utf8))
        return box.combined!.base64EncodedString()
    }

    /// Tries each Mac's key, since the visible payload doesn't say which Mac sent it.
    public static func open(_ e: String, keys: [String: SymmetricKey]) -> Notice? {
        guard let data = Data(base64Encoded: e), let box = try? AES.GCM.SealedBox(combined: data) else { return nil }
        for (mac, key) in keys {
            if let plain = try? AES.GCM.open(box, using: key, authenticating: Data(mac.utf8)),
                let notice = try? JSONDecoder().decode(Notice.self, from: plain), notice.mac == mac
            {
                return notice
            }
        }
        return nil
    }
}
