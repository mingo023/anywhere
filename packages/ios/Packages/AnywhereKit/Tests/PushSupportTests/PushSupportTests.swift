import CryptoKit
import Foundation
import Testing
@testable import PushSupport

private let notice = Notice(mac: "mac-1", agent: "a1", kind: "needsYou", title: "Approve the migration", place: "anywhere · calm-otter", line: "Bash: rm -rf build", request: "r1", badge: 2)

@Suite("Push support")
struct PushSupportTests {
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

    @Test("the sealed form is nonce, ciphertext and tag, as Go's gcm.Seal lays it out")
    func layout() throws {
        let key = SymmetricKey(size: .bits256)
        let data = try #require(Data(base64Encoded: NoticeBox.seal(notice, key: key)))
        let plain = try JSONEncoder().encode(notice)
        #expect(data.count == 12 + plain.count + 16)
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
}
