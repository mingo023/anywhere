import Foundation
import Testing
import Wire
@testable import Client

@Suite("Backoff and outcomes")
struct BackoffTests {
    @Test("redial delays double from a quarter second up to eight, jittered within each step's upper half")
    func doubles() {
        var backoff = Backoff()
        let steps = (0..<8).map { _ in backoff.next(random: 1) }
        #expect(steps == [0.25, 0.5, 1, 2, 4, 8, 8, 8])
        var low = Backoff()
        #expect(low.next(random: 0) == 0.125)
        #expect(low.next(random: 0.5) == 0.375)
    }

    @Test("a reset starts the delays over")
    func resets() {
        var backoff = Backoff()
        _ = backoff.next()
        _ = backoff.next()
        backoff.reset()
        #expect(backoff.next(random: 1) == 0.25)
    }

    @Test("close codes and refusals say whether to pair again, update a side or redial")
    func outcomes() {
        #expect(CloseOutcome.final(closeCode: 4401, errorCode: nil) == .revoked)
        #expect(CloseOutcome.final(closeCode: nil, errorCode: .notPaired) == .revoked)
        #expect(CloseOutcome.final(closeCode: 1000, errorCode: .revoked) == .revoked)
        #expect(CloseOutcome.final(closeCode: 4426, errorCode: .serverTooOld) == .updateMac)
        #expect(CloseOutcome.final(closeCode: 4426, errorCode: .clientTooOld) == .updatePhone)
        #expect(CloseOutcome.final(closeCode: 4426, errorCode: nil) == .updatePhone)
        #expect(CloseOutcome.final(closeCode: 1006, errorCode: nil) == nil)
        #expect(CloseOutcome.final(closeCode: nil, errorCode: .rateLimited) == nil)
    }
}

@Suite("Credentials")
struct CredentialsTests {
    @Test("credentials survive a round trip through their stored form")
    func roundTrip() throws {
        let c = Credentials(host: "100.64.0.1:4517", deviceToken: "tok", deviceId: "d1", macName: "Mingo's Mac", clientId: "ios-x", serverId: "mac")
        #expect(try JSONDecoder().decode(Credentials.self, from: JSONEncoder().encode(c)) == c)
        #expect(c.id == "d1")
    }

    @Test("a new client id names the platform, the time and eight random characters")
    func clientIds() {
        let id = Credentials.newClientId(now: Date(timeIntervalSince1970: 1_790_000_000))
        let parts = id.split(separator: "-")
        #expect(parts.count == 3)
        #expect(parts[0] == "ios")
        #expect(parts[1] == String(1_790_000_000_000, radix: 36))
        #expect(parts[2].count == 8)
        #expect(Credentials.newClientId() != Credentials.newClientId())
    }
}
