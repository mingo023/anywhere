import CryptoKit
import Fixtures
import Foundation
import Testing
import Wire
@testable import PushSupport

private let notice = Notice(mac: "mac-1", agent: "a1", kind: "needsYou", title: "Approve the migration", place: "anywhere · calm-otter", line: "Bash: rm -rf build", request: "r1", badge: 2)

/// pocketd's push testdata, which both sides must agree on.
enum PushGolden {
    static let root = URL(fileURLWithPath: #filePath)
        .deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()
        .deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()
        .appendingPathComponent("pocketd/internal/push/testdata")

    static func data(_ name: String) throws -> Data { try Data(contentsOf: root.appendingPathComponent(name)) }
}

private struct Vector: Decodable {
    let key, nonce, aad, plain, sealed: String
}

@Suite("Push support")
struct PushSupportTests {
    @Test("a notice pocketd sealed opens here to the same notice")
    func opensPocketdsVector() throws {
        let v = try JSONDecoder().decode(Vector.self, from: PushGolden.data("seal_vector.json"))
        let key = SymmetricKey(data: try #require(Data(base64Encoded: v.key)))
        let opened = try #require(NoticeBox.open(v.sealed, keys: ["other": NoticeBox.newKey(), v.aad: key]))
        #expect(opened == (try JSONDecoder().decode(Notice.self, from: Data(v.plain.utf8))))
        #expect(opened.title == "Fix the flaky test" && opened.request == "r1" && opened.badge == 2)
    }

    @Test("sealing with pocketd's nonce gives pocketd's bytes, so the layouts match")
    func sealsLikePocketd() throws {
        let v = try JSONDecoder().decode(Vector.self, from: PushGolden.data("seal_vector.json"))
        let key = SymmetricKey(data: try #require(Data(base64Encoded: v.key)))
        let nonce = try AES.GCM.Nonce(data: try #require(Data(base64Encoded: v.nonce)))
        let box = try AES.GCM.seal(Data(v.plain.utf8), using: key, nonce: nonce, authenticating: Data(v.aad.utf8))
        #expect(box.combined?.base64EncodedString() == v.sealed)
    }

    @Test("a sealed notice opens with its Mac's key")
    func opens() throws {
        let key = SymmetricKey(size: .bits256)
        let e = try NoticeBox.seal(notice, key: key)
        #expect(NoticeBox.open(e, keys: ["other": SymmetricKey(size: .bits256), "mac-1": key]) == notice)
    }

    @Test("a notice doesn't open with another key, another Mac's id, or garbage")
    func refuses() throws {
        let key = SymmetricKey(size: .bits256)
        let e = try NoticeBox.seal(notice, key: key)
        #expect(NoticeBox.open(e, keys: ["mac-1": SymmetricKey(size: .bits256)]) == nil)
        #expect(NoticeBox.open(e, keys: ["mac-2": key]) == nil)
        #expect(NoticeBox.open("not base64!", keys: ["mac-1": key]) == nil)
        #expect(NoticeBox.open(Data(count: 10).base64EncodedString(), keys: ["mac-1": key]) == nil)
    }

    @Test("an opened notice links to its session and names it for an iPad scene")
    func links() {
        #expect(notice.link == "anywhere://mac/mac-1/agent/a1")
        #expect(notice.userInfo == ["mac": "mac-1", "agent": "a1", "request": "r1", "kind": "needsYou", "link": "anywhere://mac/mac-1/agent/a1"])
        #expect(notice.targetContentIdentifier == "agent:a1")
        let test = Notice(mac: "mac-1", agent: "", kind: "test", title: "Anywhere", place: "Studio", line: "Works", badge: 0)
        #expect(test.link == "anywhere://mac/mac-1" && test.userInfo["agent"] == nil && test.targetContentIdentifier == "mac:mac-1")
    }

    @Test("the badge sums every Mac's last count")
    func badges() throws {
        let suite = "dev.mingo.anywhere.tests.\(UUID().uuidString)"
        let defaults = try #require(UserDefaults(suiteName: suite))
        defer { defaults.removePersistentDomain(forName: suite) }
        let ledger = BadgeLedger(defaults: defaults)
        #expect(ledger.record(mac: "m1", count: 2) == 2)
        #expect(ledger.record(mac: "m2", count: 3) == 5)
        #expect(ledger.record(mac: "m1", count: 0) == 3)
        ledger.forget(mac: "m2")
        #expect(ledger.record(mac: "m1", count: 1) == 1)
    }

    @Test("each category offers the actions pocketd's protocol names")
    func categories() {
        let byId = Dictionary(uniqueKeysWithValues: NotificationCategory.actions.map { ($0.id, $0.actions.map(\.id)) })
        #expect(byId == [
            "PERM": ["ALLOW", "DENY"], "PERM_NOTE": ["ALLOW", "DENY", "DENY_NOTE"], "QUESTION": ["OPEN"],
            "DONE": ["REPLY", "OPEN"], "FAILED": ["REPLY", "OPEN"], "AUTOMATION": ["OPEN"],
        ])
        let denyNote = NotificationCategory.actions[1].actions[2]
        #expect(denyNote.style == .destructive && denyNote.input?.button == "Deny")
    }

    @Test("an action turns into the message it sends the Mac")
    func replies() {
        let info = notice.userInfo
        #expect(NotificationReply.of(action: "ALLOW", userInfo: info, text: nil) == .resolve(requestId: "r1", decision: "allow", message: nil))
        #expect(NotificationReply.of(action: "DENY", userInfo: info, text: nil) == .resolve(requestId: "r1", decision: "deny", message: nil))
        #expect(NotificationReply.of(action: "DENY_NOTE", userInfo: info, text: "  use make clean  ") == .resolve(requestId: "r1", decision: "deny", message: "use make clean"))
        #expect(NotificationReply.of(action: "REPLY", userInfo: info, text: "ship it") == .prompt(agentId: "a1", text: "ship it"))
        #expect(NotificationReply.of(action: "REPLY", userInfo: info, text: "   ") == .none)
        #expect(NotificationReply.of(action: "ALLOW", userInfo: ["mac": "m"], text: nil) == .open)
        #expect(NotificationReply.of(action: "com.apple.UNNotificationDefaultActionIdentifier", userInfo: info, text: nil) == .open)
    }

    @Test("a handle is reused across Macs until a week before it expires")
    func handleCache() {
        var cache = HandleCache()
        let now = Date(timeIntervalSince1970: 1_800_000_000)
        let handle = PushHandle(handle: "h", secret: "s", expiresAt: 1_800_000_000 + 30 * 86400)
        cache.keep(handle, token: "t", kind: .alert, env: .production)
        #expect(cache.handle(token: "t", kind: .alert, env: .production, now: now) == handle)
        #expect(cache.handle(token: "t", kind: .start, env: .production, now: now) == nil)
        #expect(cache.handle(token: "t2", kind: .alert, env: .production, now: now) == nil)
        #expect(cache.handle(token: "t", kind: .alert, env: .production, now: now.addingTimeInterval(24 * 86400)) == nil)
    }

    @Test("a token is registered as hex with its kind and env, and the handle comes back")
    func registers() async throws {
        let session = RelayStub.session { request in
            let body = try JSONDecoder().decode([String: String].self, from: RelayStub.body(request))
            #expect(request.url?.path == "/v1/register" && request.httpMethod == "POST")
            #expect(body == ["token": "00ff10", "kind": "start", "env": "development"])
            return (200, #"{"handle":"h1","secret":"s1","expiresAt":1800000000}"#)
        }
        let relay = RelayClient(base: URL(string: "https://relay.test")!, session: session)
        let handle = try await relay.register(token: Data([0, 255, 16]).hex, kind: .start, env: .development)
        #expect(handle == PushHandle(handle: "h1", secret: "s1", expiresAt: 1_800_000_000))
    }

    @Test("a refused registration says why")
    func refusedRegistration() async {
        let session = RelayStub.session { _ in (429, #"{"reason":"RateLimited","resetsAt":"2026-10-24T10:00:00Z"}"#) }
        let relay = RelayClient(base: URL(string: "https://relay.test")!, session: session)
        await #expect(throws: RelayClient.Failure.refused(status: 429, reason: "RateLimited")) {
            try await relay.register(token: "00", kind: .alert, env: .production)
        }
    }
}
