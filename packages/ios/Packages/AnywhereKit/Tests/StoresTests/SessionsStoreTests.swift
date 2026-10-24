import Domain
import Fixtures
import Testing
import Wire
@testable import Stores

@MainActor
private func store(_ messages: [ServerMessage]) -> SessionsStore {
    let store = SessionsStore()
    for m in messages { _ = store.apply(m) }
    return store
}

private func agent(_ id: String, _ status: AgentStatus, updated: Int, created: Int = 0, failed: Bool? = nil, attached: Bool = true) -> AgentSummary {
    AgentSummary(id: id, terminalId: "t-\(id)", title: id, cwd: "/w", provider: "claude", status: status, failed: failed, attached: attached,
                 epoch: 1, maxSeq: 0, createdAt: created, updatedAt: updated)
}

@Suite("Sessions store")
@MainActor
struct SessionsStoreTests {
    @Test("an agent list fills the sessions, Up next and the newest-first list")
    func list() {
        let s = store([.agentList(.init(agents: Sample.agents))])
        #expect(s.sessions.count == Sample.agents.count)
        #expect(s.upNext == ["a-needs-old", "a-needs", "a-failed", "a-done"])
        #expect(s.recent.first == "a-working")
        #expect(Set(s.recent) == Set(Sample.agents.map(\.id)))
        #expect(s.session("a-detached")?.status == .idle)
    }

    @Test("an update changes one session in place and moves it in Up next")
    func update() {
        let s = store([.agentList(.init(agents: [agent("a", .working, updated: 1), agent("b", .done, updated: 2)]))])
        let a = s.session("a")
        #expect(s.upNext == ["b"])
        #expect(s.apply(.agentUpdate(.init(agent: agent("a", .needsYou, updated: 3)))))
        #expect(s.session("a") === a, "the same model, so only its card redraws")
        #expect(a?.status == .needsYou)
        #expect(s.upNext == ["a", "b"])
    }

    @Test("an update that changes nothing reports nothing")
    func sameUpdate() {
        let s = store([.agentList(.init(agents: [agent("a", .working, updated: 1)]))])
        #expect(!s.apply(.agentUpdate(.init(agent: agent("a", .working, updated: 1)))))
    }

    @Test("an update for an agent not yet listed adds it")
    func newAgent() {
        let s = store([.agentUpdate(.init(agent: agent("n", .done, updated: 5, failed: true)))])
        #expect(s.session("n")?.status == .failed)
        #expect(s.upNext == ["n"])
    }

    @Test("a closed agent leaves the list, and so do agents a new list doesn't have")
    func leaves() {
        let s = store([.agentList(.init(agents: [agent("a", .done, updated: 1), agent("b", .done, updated: 2), agent("c", .idle, updated: 3)]))])
        _ = s.apply(.agentUpdate(.init(agent: agent("a", .closed, updated: 4))))
        #expect(s.session("a") == nil)
        #expect(s.upNext == ["b"])
        _ = s.apply(.agentList(.init(agents: [agent("c", .idle, updated: 3)])))
        #expect(Array(s.sessions.keys) == ["c"])
        #expect(s.upNext.isEmpty)
        #expect(s.recent == ["c"])
    }

    @Test("an ask lands on its session until it is resolved")
    func asks() {
        let s = store(Sample.snapshot)
        #expect(s.requests.count == 2)
        #expect(s.session("a-needs")?.requests == [Sample.permission])
        #expect(s.apply(.permissionResolved(.init(requestId: "r-needs", decision: .allow))))
        #expect(s.session("a-needs")?.requests == [])
        #expect(s.requests.keys.sorted() == ["r-question"])
        #expect(!s.apply(.permissionResolved(.init(requestId: "r-needs", decision: .allow))))
    }

    @Test("an ask that comes before its agent is listed joins it when it is")
    func earlyAsk() {
        let s = store([.permissionRequest(.init(request: Sample.permission))])
        _ = s.apply(.agentList(.init(agents: Sample.agents)))
        #expect(s.session("a-needs")?.requests == [Sample.permission])
    }

    @Test("asks held from before a reconnect are dropped, since the snapshot re-sends the open ones")
    func reconnect() {
        let s = store(Sample.snapshot)
        s.connected(Sample.hello)
        #expect(s.requests.isEmpty)
        #expect(s.session("a-needs")?.requests == [])
        _ = s.apply(.permissionRequest(.init(request: Sample.permission)))
        #expect(s.session("a-needs")?.requests == [Sample.permission])
    }

    @Test("an agent that leaves takes its asks with it")
    func orphanAsks() {
        let s = store(Sample.snapshot)
        _ = s.apply(.agentUpdate(.init(agent: Sample.agents[0].with(status: .closed))))
        #expect(s.requests["r-needs"] == nil)
    }

    @Test("naming failed marks its session once")
    func namingFailed() {
        let s = store([.agentList(.init(agents: Sample.agents))])
        #expect(s.apply(.namingFailed(.init(agentId: "a-idle"))))
        #expect(s.session("a-idle")?.namingFailed == true)
        #expect(!s.apply(.namingFailed(.init(agentId: "a-idle"))))
    }

    @Test("messages for other stores are left alone")
    func ignores() {
        let s = SessionsStore()
        #expect(!s.apply(.gitChanged(.init(tree: "/w", rev: "r"))))
        #expect(!s.apply(.unknown("agent.sparkle")))
    }
}

extension AgentSummary {
    func with(status: AgentStatus) -> AgentSummary {
        var copy = self
        copy.status = status
        return copy
    }
}
