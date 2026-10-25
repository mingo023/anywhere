import Client
import Fixtures
import Testing
import Wire
@testable import Stores

@MainActor
private func host() -> HostModel {
    HostModel(credentials: Credentials(host: "127.0.0.1:9", deviceToken: "t", deviceId: "d1", macName: "Saved name", clientId: "ios-test"))
}

@Suite("Host model")
@MainActor
struct HostModelTests {
    @Test("hello.ok puts the Mac online under its own name, and the snapshot reaches the sessions")
    func connects() {
        let h = host()
        #expect(h.name == "Saved name")
        h.apply([.connected(Sample.hello)] + Sample.snapshot.map(ConnectionEvent.message))
        #expect(h.link == .online)
        #expect(h.name == "Mingo's MacBook Pro")
        #expect(h.sessions.sessions.count == Sample.agents.count)
        #expect(h.sessions.requests.count == 2)
    }

    @Test("a drop shows the redial, and a revoke ends the link")
    func closes() {
        let h = host()
        h.apply([.connected(Sample.hello), .closed(.retrying(seconds: 0.5))])
        #expect(h.link == .retrying(seconds: 0.5))
        h.apply([.closed(.revoked)])
        #expect(h.link == .ended(.revoked))
    }

    @Test("a chat is made once per agent and dropped on close")
    func chats() {
        let h = host()
        let chat = h.chat("a1")
        #expect(h.chat("a1") === chat)
        #expect(chat.agentId == "a1")
        h.closeChat("a1")
        #expect(h.chats.isEmpty)
    }

    @Test("a message names the agent it is about, so only that agent's chat sees it")
    func agentIds() {
        #expect(ServerMessage.agentStream(.init(agentId: "a1", epoch: 1, item: Sample.timeline(turns: 1).items[0])).agentId == "a1")
        #expect(ServerMessage.agentTimeline(Sample.timeline(agentId: "a2", turns: 1)).agentId == "a2")
        #expect(ServerMessage.permissionRequest(.init(request: Sample.permission)).agentId == "a-needs")
        #expect(ServerMessage.agentList(.init(agents: [])).agentId == nil)
    }

    @Test("a message no store takes is reported as untaken")
    func untaken() {
        let h = host()
        #expect(!h.route(.unknown("agent.sparkle")))
        #expect(h.route(.agentList(.init(agents: Sample.agents))))
    }
}

@Suite("App model")
@MainActor
struct AppModelTests {
    @Test("a Mac is added once and found by the serverId it said hello with")
    func hosts() {
        let app = AppModel()
        let creds = Credentials(host: "127.0.0.1:9", deviceToken: "t", deviceId: "d1", macName: "Mac", clientId: "c", serverId: "saved-id")
        let h = app.add(creds, start: false)
        #expect(app.add(creds, start: false) === h)
        #expect(app.host(mac: "saved-id") === h)
        h.apply([.connected(Sample.hello)])
        #expect(app.host(mac: Sample.mac) === h)
        app.remove("d1")
        #expect(app.hosts.isEmpty)
    }
}
