import ActivityModel
import Client
import Fixtures
import Foundation
import PushSupport
import Testing
import Wire
@testable import Stores

/// The kinds the stub relay was asked to seal, in order.
private final class Calls: @unchecked Sendable {
    private let lock = NSLock()
    private var kinds: [String] = []
    func add(_ kind: String) { lock.withLock { kinds.append(kind) } }
    var all: [String] { lock.withLock { kinds } }
}

/// A center on its own defaults and keychain service, sealing tokens with a stub relay that counts its calls.
@MainActor
private func center(_ calls: Calls = Calls()) -> PushCenter {
    let id = UUID().uuidString
    let session = RelayStub.session { request in
        let body = try JSONDecoder().decode([String: String].self, from: RelayStub.body(request))
        calls.add(body["kind"]!)
        return (200, #"{"handle":"h-\#(body["kind"]!)","secret":"s","expiresAt":4102444800}"#)
    }
    return PushCenter(
        defaults: UserDefaults(suiteName: "push-test-\(id)")!, keys: Keychain(service: "dev.mingo.anywhere.push.test-\(id)"),
        relay: RelayClient(base: URL(string: "https://relay.test")!, session: session), env: .development)
}

@Suite("Push")
@MainActor
struct PushStoreTests {
    @Test("nothing registers until notifications are allowed and the token has come")
    func waits() async throws {
        let c = center()
        #expect(try await c.registration(mac: Sample.mac) == nil)
        c.received(alertToken: Data([1, 2]))
        #expect(try await c.registration(mac: Sample.mac) == nil)
        c.granted(false)
        #expect(try await c.registration(mac: Sample.mac) == nil)
    }

    @Test("a registration carries the sealed handles, this Mac's key every time, and the prefs")
    func registers() async throws {
        let c = center()
        defer { c.forget(mac: Sample.mac) }
        c.granted(true)
        c.received(alertToken: Data([1, 2]))
        c.received(startToken: Data([3]))
        let first = try #require(try await c.registration(mac: Sample.mac))
        #expect(first.alert.handle == "h-alert" && first.start?.handle == "h-start")
        #expect(first.env == "development" && first.prefs == PushCenter.defaultPrefs)
        #expect(Data(base64Encoded: first.key)?.count == 32)
        #expect(try await c.registration(mac: Sample.mac)?.key == first.key)
    }

    @Test("several Macs registering at once ask the relay once per token")
    func sealsOnce() async throws {
        let calls = Calls()
        let c = center(calls)
        defer { for mac in ["m1", "m2", "m3"] { c.forget(mac: mac) } }
        c.granted(true)
        c.received(alertToken: Data([1]))
        async let a = c.registration(mac: "m1")
        async let b = c.registration(mac: "m2")
        async let d = c.registration(mac: "m3")
        _ = try await (a, b, d)
        #expect(calls.all == ["alert"])
    }

    @Test("with Live Activities off, a Mac gets no start handle")
    func noStartWhenOff() async throws {
        let c = center()
        defer { c.forget(mac: Sample.mac) }
        c.granted(true)
        c.received(alertToken: Data([1]))
        c.received(startToken: Data([2]))
        c.set(liveActivities: false)
        #expect(try await c.registration(mac: Sample.mac)?.start == nil)
    }

    @Test("prefs outlive the app")
    func prefsPersist() {
        let c = center()
        var prefs = c.prefs
        prefs.done = false
        prefs.details = true
        c.set(prefs)
        c.set(liveActivities: false)
        let again = PushCenter(defaults: c.defaults, keys: c.keys, env: .development)
        #expect(again.prefs == prefs && !again.liveActivities)
    }

    @Test("a Mac without push.v1 says so, and one with it waits for permission")
    func states() async {
        let c = center()
        let old = PushStore(center: c)
        var hello = Sample.hello
        hello.caps = hello.caps?.filter { $0 != Caps.push }
        old.connected(hello)
        #expect(old.state == .unsupported)
        let store = PushStore(connection: DaemonConnection(config: .init(host: "127.0.0.1:9", token: "t", clientId: "c")), center: c)
        store.connected(Sample.hello)
        await store.register()
        #expect(store.state == .waiting)
    }

    @Test("sessions count as pocketd counts them: attached, and Needs you, working, Failed or Done")
    func counts() {
        let sessions = PushStore.sessions(Sample.agents)
        let byId = Dictionary(uniqueKeysWithValues: sessions.map { ($0.id, $0.status) })
        #expect(byId["a-needs"] == "needsYou" && byId["a-failed"] == "failed" && byId["a-done"] == "done" && byId["a-working"] == "working")
        #expect(byId["a-idle"] == nil && byId["a-detached"] == nil)
    }

    @Test("the widget's titles name each session and where it runs, as the alerts do")
    func titles() {
        var shell = Sample.agents[0]
        shell.title = ""
        shell.provider = "shell"
        shell.project = nil
        shell.id = "x"
        let titles = PushStore.titles([Sample.agents[0], shell])
        #expect(titles["a-needs"] == ActivityTitles.Entry(title: "Approve the migration", place: "anywhere · calm-otter", provider: "claude"))
        #expect(titles["x"] == ActivityTitles.Entry(title: "Shell", place: "", provider: "shell"))
    }

    @Test("the snapshot saves titles for the widget, counts the badge, and starts the activity once the Mac turns busy")
    func snapshot() {
        let c = center()
        var started: [MacActivity] = []
        c.startActivity = { attributes, _ in started.append(attributes) }
        let store = PushStore(center: c)
        store.connected(Sample.hello)
        _ = store.apply(.agentList(.init(agents: Sample.agents)))
        _ = store.apply(.agentUpdate(.init(agent: Sample.agents[4])))
        #expect(started == [MacActivity(mac: Sample.mac, macName: "Mingo's MacBook Pro")])
        #expect(ActivityTitles(defaults: c.defaults).entry(mac: Sample.mac, slot: "a-working").title == "Port the commit graph lanes")
        #expect(BadgeLedger(defaults: c.defaults).record(mac: "other", count: 0) == 4)
    }
}

@Suite("Delivered alerts")
struct DeliveredAlertTests {
    @Test("a sync clears a Mac's alerts for sessions no longer needing a look, and nothing else")
    func outdated() {
        let keep: Set = ["a1"]
        #expect(PushCenter.outdated(mac: "m", agent: "a2", kind: "done", on: "m", keep: keep))
        #expect(!PushCenter.outdated(mac: "m", agent: "a1", kind: "needsYou", on: "m", keep: keep))
        #expect(!PushCenter.outdated(mac: "other", agent: "a2", kind: "done", on: "m", keep: keep))
        #expect(!PushCenter.outdated(mac: "m", agent: "a2", kind: "automation", on: "m", keep: keep))
        #expect(!PushCenter.outdated(mac: "m", agent: nil, kind: "test", on: "m", keep: keep))
    }
}
