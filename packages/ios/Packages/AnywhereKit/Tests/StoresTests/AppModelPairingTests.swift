import Client
import Fixtures
import Foundation
import Testing
import Wire
@testable import Stores

private let link = PairingLink(host: "100.64.0.1:4517", code: "q3xYq3xYq3xYq3xYq3xY_-", name: "Mingo's MacBook Pro")

@Suite("Pairing and the saved Macs")
@MainActor
struct AppModelPairingTests {
    @Test("pairing saves the Mac's token under its device id and connects to it")
    func pairs() throws {
        let vault = MemoryVault()
        let app = AppModel(vault: vault)
        let host = app.paired(PairOk(deviceId: "d1", token: "tok"), link: link)
        defer { app.forget("d1") }
        #expect(app.hosts.map(\.id) == ["d1"])
        #expect(host.credentials.host == "100.64.0.1:4517")
        let saved = try #require(try vault.all().first)
        #expect(saved.deviceToken == "tok")
        #expect(saved.macName == "Mingo's MacBook Pro")
        #expect(saved.clientId.hasPrefix("ios-"))
    }

    @Test("a Mac's serverId and name are saved once it says hello, so a deep link finds it before it next connects")
    func remembersServerId() async throws {
        let vault = MemoryVault()
        let app = AppModel(vault: vault)
        let host = app.add(Credentials(host: link.host, deviceToken: "t", deviceId: "d1", macName: "Old name", clientId: "c"), start: false)
        host.apply([.connected(Sample.hello)])
        try await settle { (try? vault.all().first?.serverId) == Sample.mac }
        #expect(try vault.all().first?.macName == "Mingo's MacBook Pro")

        let restored = AppModel.restore(from: vault)
        defer { restored.forget("d1") }
        #expect(restored.host(mac: Sample.mac)?.id == "d1")
    }

    @Test("a revoked Mac loses its token but keeps its host, and comes back waiting for a new code")
    func revoked() async throws {
        let vault = MemoryVault()
        let app = AppModel(vault: vault)
        let host = app.add(Credentials(host: link.host, deviceToken: "t", deviceId: "d1", macName: "Mac", clientId: "c"), start: false)
        host.apply([.connected(Sample.hello), .closed(.revoked)])
        try await settle { (try? vault.all().first?.deviceToken) == "" }
        #expect(try vault.all().first?.host == link.host)

        let restored = AppModel.restore(from: vault)
        #expect(restored.hosts.first?.link == .ended(.revoked))
        #expect(restored.hosts.first?.reachability(at: Date()) == .revoked)
    }

    @Test("pairing again with a Mac replaces its old record and keeps this phone's client id")
    func repairs() throws {
        let vault = MemoryVault([Credentials(host: link.host, deviceToken: "", deviceId: "old", macName: "Mac", clientId: "ios-keep", serverId: "s1")])
        let app = AppModel.restore(from: vault)
        let host = app.paired(PairOk(deviceId: "new", token: "tok"), link: link)
        defer { app.forget("new") }
        #expect(app.hosts.map(\.id) == ["new"])
        #expect(try vault.all().map(\.deviceId) == ["new"])
        #expect(host.credentials.clientId == "ios-keep")
        #expect(host.credentials.serverId == "s1")
    }

    @Test("forgetting a Mac drops it and its record")
    func forgets() throws {
        let vault = MemoryVault()
        let app = AppModel(vault: vault)
        app.paired(PairOk(deviceId: "d1", token: "tok"), link: link)
        app.forget("d1")
        #expect(app.hosts.isEmpty)
        #expect(try vault.all().isEmpty)
    }

    @Test("a Mac to update is dialed again with a fresh connection on restart")
    func restarts() {
        let app = AppModel()
        let host = app.add(Credentials(host: "127.0.0.1:9", deviceToken: "t", deviceId: "d1", macName: "Mac", clientId: "c"), start: false)
        host.apply([.closed(.updateMac)])
        app.restart("d1")
        defer { app.forget("d1") }
        #expect(app.hosts.first !== host)
        #expect(app.hosts.first?.link == .connecting)
    }
}

@Suite("Reachability")
@MainActor
struct ReachabilityTests {
    @Test("a drop shows nothing for 4 s, then reconnecting, then unreachable after 30 s", arguments: [
        (0.0, Reachability.connecting), (3.9, .connecting), (4, .reconnecting), (29.9, .reconnecting), (30, .unreachable), (600, .unreachable),
    ])
    func banners(_ seconds: Double, _ want: Reachability) {
        #expect(Reachability(link: .retrying(seconds: 1), offlineFor: seconds) == want)
        #expect(Reachability(link: .connecting, offlineFor: seconds) == want)
    }

    @Test("an ended link reads as what ended it, however long ago")
    func ended() {
        #expect(Reachability(link: .ended(.revoked), offlineFor: 1) == .revoked)
        #expect(Reachability(link: .ended(.updateMac), offlineFor: 100) == .updateMac)
        #expect(Reachability(link: .ended(.updatePhone), offlineFor: 0) == .updatePhone)
        #expect(Reachability(link: .ended(.stopped), offlineFor: 0) == .stopped)
        #expect(Reachability.revoked.isFinal && !Reachability.unreachable.isFinal)
    }

    @Test("the reading changes next at 4 s and 30 s, and not after")
    func nextChange() {
        #expect(Reachability.nextChange(offlineFor: 1) == 3)
        #expect(Reachability.nextChange(offlineFor: 10) == 20)
        #expect(Reachability.nextChange(offlineFor: 31) == nil)
    }

    @Test("a host is offline from its first dial and from each drop until hello.ok")
    func offlineSince() {
        var clock = Date(timeIntervalSince1970: 1000)
        let host = HostModel(
            credentials: Credentials(host: "127.0.0.1:9", deviceToken: "t", deviceId: "d1", macName: "Mac", clientId: "c"), now: { clock })
        #expect(host.reachability(at: clock.addingTimeInterval(5)) == .reconnecting)
        host.apply([.connected(Sample.hello)])
        #expect(host.offlineSince == nil)
        #expect(host.reachability(at: clock) == .online)
        clock = clock.addingTimeInterval(60)
        host.apply([.closed(.retrying(seconds: 0.25)), .closed(.retrying(seconds: 0.5))])
        #expect(host.offlineSince == clock, "the second drop keeps the first one's time")
        #expect(host.reachability(at: clock.addingTimeInterval(31)) == .unreachable)
    }

    @Test("host.changed updates the Mac's Tailscale and keep-awake state")
    func hostChanged() {
        let host = HostModel(credentials: Credentials(host: "127.0.0.1:9", deviceToken: "t", deviceId: "d1", macName: "Mac", clientId: "c"))
        host.apply([.connected(Sample.hello)])
        #expect(host.hostState == HostState(tailnet: true, keepingAwake: true))
        host.apply([.message(.hostChanged(.init(host: HostState(tailnet: false, keepingAwake: true))))])
        #expect(host.hostState?.tailnet == false)
    }
}

/// Waits for the model's observation hop to land.
@MainActor
private func settle(_ done: () -> Bool) async throws {
    for _ in 0..<100 where !done() { try await Task.sleep(for: .milliseconds(5)) }
    #expect(done())
}
