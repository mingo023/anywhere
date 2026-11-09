import Client
import Fixtures
import Loopback
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

    @Test("an ask answered from the list reaches pocketd with its request and decision, and throws while offline")
    func answers() async throws {
        await #expect(throws: DaemonError.self) { try await host().answer(Sample.permission, .allow) }
        let server = try await TestServer { peer, frame in
            if case .hello = frame.message { peer.send(ServerFrame(id: frame.id, .helloOk(Sample.hello))) }
        }
        defer { server.stop() }
        let connection = DaemonConnection(config: .init(host: server.host, token: "t", clientId: "c"))
        await connection.start()
        defer { Task { await connection.stop() } }
        for _ in 0..<200 where await !connection.isOnline { try await Task.sleep(for: .milliseconds(5)) }
        let h = HostModel(credentials: Credentials(host: server.host, deviceToken: "t", deviceId: "d1", macName: "Mac", clientId: "c"), connection: connection)
        try await h.answer(Sample.permission, .deny)
        let answers = { server.received.compactMap { if case .permissionResolve(let r) = $0.message { r } else { nil } } }
        for _ in 0..<200 where answers().isEmpty { try await Task.sleep(for: .milliseconds(5)) }
        #expect(answers().map(\.requestId) == ["r-needs"])
        #expect(answers().map(\.decision) == [.deny])
    }

    @Test("a drop shows the redial, and a revoke ends the link")
    func closes() {
        let h = host()
        h.apply([.connected(Sample.hello), .closed(.retrying(seconds: 0.5))])
        #expect(h.link == .retrying(seconds: 0.5))
        h.apply([.closed(.revoked)])
        #expect(h.link == .ended(.revoked))
    }

    @Test("a launch that starts tells the chats, so the one whose /new asked for it lets its draft go")
    func startTellsChats() {
        let h = host()
        h.apply([.connected(Sample.hello)] + Sample.snapshot.map(ConnectionEvent.message))
        let chat = h.chat("a-needs")
        chat.draft.text = "/new fix the build"
        chat.startNewSession("fix the build")
        chat.openedNewSession()
        let spec = LaunchSpec(project: "/p", checkout: .worktree("/p"), provider: .claude, access: .edits, plan: false, prompt: "fix the build")
        h.workspace.add(LaunchPlan(spec: spec), requestId: "r1", title: "", place: "/p")
        h.apply([.message(.agentCreating(.init(requestId: "r1", terminalId: "t-a-working", cwd: "/p", setup: false))),
                 .message(.agentList(.init(agents: Sample.agents)))])
        #expect(chat.draft.isEmpty)
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

    @Test("a chat's store goes once no stack shows it, or once its session closes or goes unlisted")
    func releasesChats() {
        let h = host()
        h.apply([.connected(Sample.hello)] + Sample.snapshot.map(ConnectionEvent.message))
        #expect(h.agentsListed)
        let ids = Array(h.sessions.sessions.keys.sorted().prefix(3))
        for id in ids + ["gone"] { _ = h.chat(id) }
        h.keepChats(Set(ids))
        #expect(Set(h.chats.keys) == Set(ids))
        var closed = h.sessions.session(ids[0])!.summary
        closed.status = .closed
        h.apply([.message(.agentUpdate(.init(agent: closed)))])
        #expect(Set(h.chats.keys) == Set(ids.dropFirst()))
        h.apply([.message(.agentList(.init(agents: Sample.agents.filter { $0.id != ids[1] })))])
        #expect(Set(h.chats.keys) == [ids[2]])
    }

    @Test("a message names the agent it is about, so only that agent's chat sees it")
    func agentIds() {
        #expect(ServerMessage.agentStream(.init(agentId: "a1", epoch: 1, item: Sample.timeline(turns: 1).items[0])).agentId == "a1")
        #expect(ServerMessage.agentTimeline(Sample.timeline(agentId: "a2", turns: 1)).agentId == "a2")
        #expect(ServerMessage.permissionRequest(.init(request: Sample.permission)).agentId == "a-needs")
        #expect(ServerMessage.agentList(.init(agents: [])).agentId == nil)
    }

    @Test("with front.v1 the scene says outright whether the app is in front, with the chats on screen")
    func sceneFront() {
        #expect(HostModel.sceneView(active: true, viewing: ["a1"], front: true) == .init(agentIds: ["a1"], front: true))
        #expect(HostModel.sceneView(active: true, viewing: [], front: true) == .init(agentIds: [], front: true))
        #expect(HostModel.sceneView(active: false, viewing: ["a1"], front: true) == .init(agentIds: [], front: false))
    }

    @Test("without front.v1 the background is no ids, and the front re-sends only a chat on screen")
    func sceneWithoutFront() {
        #expect(HostModel.sceneView(active: false, viewing: ["a1"], front: false) == .init(agentIds: []))
        #expect(HostModel.sceneView(active: true, viewing: ["a1"], front: false) == .init(agentIds: ["a1"]))
        #expect(HostModel.sceneView(active: true, viewing: [], front: false) == nil)
    }

    @Test("a hello while the app is in the background still says so, with nothing viewed")
    func helloInBackground() {
        let h = host()
        h.apply([.connected(Sample.hello)])
        h.chat("a1").appear()
        #expect(h.sceneView(front: true) == .init(agentIds: ["a1"], front: true))
        h.scene(active: false)
        #expect(h.sceneView(front: true) == .init(agentIds: [], front: false))
        #expect(h.sceneView(front: false) == .init(agentIds: []))
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
        app.forget("d1")
        #expect(app.hosts.isEmpty)
    }
}
