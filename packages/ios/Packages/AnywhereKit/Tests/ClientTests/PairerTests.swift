import Foundation
import Testing
import Wire
@testable import Client

private let code = "q3xYq3xYq3xYq3xYq3xY_-"

@Suite("Pairer", .serialized)
struct PairerTests {
    @Test("a right code is traded for the device's id and token, sending the phone's name, platform and protocol range")
    func pairs() async throws {
        let server = try await TestServer { peer, frame in
            peer.send(ServerFrame(id: frame.id, .pairOk(.init(deviceId: "d1", token: "tok"))))
            peer.close(code: 1000)
        }
        defer { server.stop() }
        let ok = try await Pairer.pair(host: server.host, code: code, phoneName: "Mingo's iPhone")
        #expect(ok == PairOk(deviceId: "d1", token: "tok"))
        let sent = try #require(server.received.first)
        #expect(sent.message == .pair(.init(code: code, name: "Mingo's iPhone", platform: "ios", protocol: .phone)))
    }

    @Test("each refusal the Mac sends is told apart", arguments: [
        (ErrorCode.pairExpired, PairingFailure.expired), (.pairUsed, .used), (.pairInvalid, .invalid), (.pairLocked, .locked),
        (.rateLimited, .rateLimited), (.clientTooOld, .updatePhone), (.serverTooOld, .updateMac),
    ])
    func refusals(_ error: ErrorCode, _ failure: PairingFailure) async throws {
        let server = try await TestServer { peer, frame in
            peer.send(ServerFrame(id: frame.id, .error(.init(message: "no", code: error))))
        }
        defer { server.stop() }
        await #expect(throws: failure) { try await Pairer.pair(host: server.host, code: code, phoneName: "iPhone") }
    }

    @Test("a Mac too old to read the pair message is a Mac to update")
    func oldMac() async throws {
        let server = try await TestServer { peer, frame in
            peer.send(ServerFrame(id: frame.id, .error(.init(message: malformedMessage))))
        }
        defer { server.stop() }
        await #expect(throws: PairingFailure.updateMac) { try await Pairer.pair(host: server.host, code: code, phoneName: "iPhone") }
    }

    @Test("a 4426 close without a reply names the side to update by its reason")
    func versionClose() async throws {
        let server = try await TestServer { peer, _ in peer.close(code: 4426, reason: "server_too_old") }
        defer { server.stop() }
        await #expect(throws: PairingFailure.updateMac) { try await Pairer.pair(host: server.host, code: code, phoneName: "iPhone") }
    }

    @Test("a Mac that never answers is unreachable once the timeout passes")
    func silent() async throws {
        let server = try await TestServer { _, _ in }
        defer { server.stop() }
        let start = Date()
        await #expect(throws: PairingFailure.unreachable) {
            try await Pairer.pair(host: server.host, code: code, phoneName: "iPhone", timeout: .milliseconds(300))
        }
        #expect(Date().timeIntervalSince(start) < 3)
    }

    @Test("nothing listening is unreachable")
    func nothingThere() async throws {
        await #expect(throws: PairingFailure.unreachable) {
            try await Pairer.pair(host: "127.0.0.1:9", code: code, phoneName: "iPhone", timeout: .seconds(2))
        }
    }

    @Test("the memory vault keeps, replaces and forgets credentials by device id")
    func vault() throws {
        let vault = MemoryVault()
        let mac = Credentials(host: "100.64.0.1:4517", deviceToken: "t", deviceId: "d1", macName: "Mac", clientId: "c")
        try vault.save(mac)
        var renamed = mac
        renamed.macName = "Studio"
        try vault.save(renamed)
        #expect(try vault.all() == [renamed])
        try vault.remove(deviceId: "d1")
        #expect(try vault.all().isEmpty)
    }
}
