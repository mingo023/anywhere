import Foundation
import Fixtures
import Testing
import Wire
@testable import Stores

private func agent(_ status: AgentStatus = .idle, resets: Int? = nil) -> AgentSummary {
    AgentSummary(id: "a1", terminalId: "t-a1", title: "a1", cwd: "/w", provider: "claude", status: status, attached: true,
                 epoch: 1, maxSeq: 0, resets: resets, createdAt: 0, updatedAt: 0)
}

private func item(_ id: String, _ seq: Int, _ text: String = "hi") -> TimelineItem {
    TimelineItem(id: id, seq: seq, ts: seq * 1000, body: .assistant(text: text))
}

private func timeline(_ items: [TimelineItem], hasOlder: Bool = false) -> ServerMessage {
    .agentTimeline(.init(agentId: "a1", epoch: 1, items: items, hasOlder: hasOlder, maxSeq: items.last?.seq ?? 0))
}

private func stream(_ item: TimelineItem) -> ServerMessage { .agentStream(.init(agentId: "a1", epoch: 1, item: item)) }

@MainActor
private func chat(_ messages: [ServerMessage], defaults: UserDefaults? = nil) -> ChatStore {
    let chat = ChatStore(agentId: "a1", defaults: defaults)
    chat.seed(agent(), requests: [], hello: Sample.hello)
    for m in messages { _ = chat.apply(m) }
    chat.flush()
    return chat
}

private func scratchDefaults() -> UserDefaults {
    let name = "chat-store-\(UUID().uuidString)"
    let d = UserDefaults(suiteName: name)!
    d.removePersistentDomain(forName: name)
    return d
}


@Suite("Chat store")
@MainActor
struct ChatStoreTests {
    @Test("stream items replace the one with their id and keep seq order")
    func upsert() {
        let c = chat([timeline([item("i1", 1), item("i3", 3)]), stream(item("i2", 2)), stream(item("i3", 3, "edited"))])
        #expect(c.items.map(\.id) == ["i1", "i2", "i3"])
        #expect(c.items.last?.body == .assistant(text: "edited"))
        #expect(c.index == ["i1": 0, "i2": 1, "i3": 2])
    }

    @Test("another agent's stream is not taken")
    func otherAgent() {
        let c = chat([])
        #expect(!c.apply(.agentStream(.init(agentId: "a2", epoch: 1, item: item("i1", 1)))))
        #expect(c.items.isEmpty)
    }

    @Test("a timeline pocketd started over drops what was held")
    func resets() {
        let c = chat([timeline([item("i1", 1), item("i2", 2)])])
        #expect(c.apply(.agentUpdate(.init(agent: agent(resets: 1)))))
        #expect(c.items.isEmpty)
        #expect(c.index.isEmpty)
    }

    @Test("a timeline pocketd started over clears the rows and takes the next page's word on older items")
    func resetsClearRows() {
        let c = chat([timeline([item("i1", 1), item("i2", 2)], hasOlder: true)])
        #expect(!c.session.rows.isEmpty)
        _ = c.apply(.agentUpdate(.init(agent: agent(resets: 1))))
        c.flush()
        #expect(c.session.rows.isEmpty)
        #expect(!c.hasOlder)
        _ = c.apply(timeline([item("j1", 1)], hasOlder: false))
        c.flush()
        #expect(c.items.map(\.id) == ["j1"])
        #expect(!c.hasOlder)
    }

    @Test("another agent's update leaves this chat's summary alone")
    func otherAgentUpdate() {
        let c = chat([timeline([item("i1", 1)])])
        var other = agent(.working, resets: 5)
        other.id = "a2"
        #expect(!c.apply(.agentUpdate(.init(agent: other))))
        #expect(c.summary?.id == "a1")
        #expect(c.items.map(\.id) == ["i1"])
    }

    @Test("a phone that may approve but not drive answers asks but can't prompt")
    func approveOnly() {
        let c = ChatStore(agentId: "a1")
        var hello = Sample.hello
        hello.scopes = [.observe, .approve]
        let ask = PermissionRequest(requestId: "r1", agentId: "a1", toolName: "Bash", detail: .shell(command: "ls", description: nil))
        c.seed(agent(), requests: [ask], hello: hello)
        #expect(c.mode == .permission(answerable: true))
        hello.scopes = [.observe, .drive]
        c.seed(agent(), requests: [ask], hello: hello)
        #expect(c.mode == .permission(answerable: false))
    }

    @Test("retrying a turn sends its prompt and keeps what's typed")
    func retryTurnKeepsDraft() {
        let c = chat([timeline([TimelineItem(id: "u1", seq: 1, ts: 1000, body: .user(text: "Port the lanes")), item("i2", 2),
                            TimelineItem(id: "r3", seq: 3, ts: 3000, body: .result(ok: true, durationMs: 2000, error: nil, usage: nil))])])
        c.draft.text = "half a thought"
        c.retryTurn(0)
        #expect(c.draft.text == "half a thought")
        #expect(c.session.outgoing.list.map(\.text) == ["Port the lanes"])
        #expect(c.sends == 1)
    }

    @Test("a saved draft leaves out image thumbnails")
    func draftsDropThumbnails() {
        let d = scratchDefaults()
        let c = chat([], defaults: d)
        c.draft.images = [.init(id: "x", name: "a.png", path: "/home/attachments/a.png", thumbnail: Data(count: 1 << 20))]
        let saved = chat([], defaults: d).draft.images
        #expect(saved.map(\.path) == ["/home/attachments/a.png"])
        #expect(saved.first?.thumbnail == nil)
    }

    @Test("an older page goes before what's held and says whether more exist")
    func older() {
        let c = chat([timeline([item("i5", 5), item("i6", 6)], hasOlder: true)])
        #expect(c.hasOlder)
        c.page(.init(agentId: "a1", epoch: 1, items: [item("i3", 3), item("i4", 4)], hasOlder: false, maxSeq: 6), older: true)
        #expect(c.items.map(\.seq) == [3, 4, 5, 6])
        #expect(!c.hasOlder)
    }

    @Test("a later latest page keeps saying more exist")
    func refetch() {
        let c = chat([timeline([item("i5", 5)], hasOlder: true), timeline([item("i5", 5), item("i6", 6)])])
        #expect(c.hasOlder)
        #expect(c.items.map(\.seq) == [5, 6])
    }

    @Test("asks queue in order and a question opens the wizard until resolved")
    func asks() {
        let c = chat([.permissionRequest(.init(request: Sample.chatQuestion(agentId: "a1")))])
        #expect(c.requests.map(\.requestId) == ["r-wizard-a1"])
        #expect(c.wizard != nil)
        #expect(c.mode != .prompt)
        #expect(c.apply(.permissionResolved(.init(requestId: "r-wizard-a1", decision: .allow))))
        #expect(c.requests.isEmpty)
        #expect(c.wizard == nil)
        #expect(c.mode == .prompt)
    }

    @Test("an observe-only phone can't prompt")
    func observeOnly() {
        let c = ChatStore(agentId: "a1")
        var hello = Sample.hello
        hello.scopes = [.observe]
        c.seed(agent(), requests: [], hello: hello)
        #expect(c.observeOnly)
        #expect(c.mode != .prompt)
    }

    @Test("a send to an idle agent goes out, clears the draft and fails when there's no Mac")
    func send() async {
        let c = chat([])
        c.draft.text = "Port the lanes"
        c.send()
        #expect(c.draft.isEmpty)
        #expect(c.sends == 1)
        #expect(c.session.outgoing.list.map(\.text) == ["Port the lanes"])
        #expect(c.session.outgoing.list.first?.delivery == .sending)
        for _ in 0..<5 { await Task.yield() }
        #expect(c.session.outgoing.list.first?.delivery == .failed)
    }

    @Test("a send while working steers into the turn")
    func steer() {
        let c = chat([.agentUpdate(.init(agent: agent(.working)))])
        c.draft.text = "also the tests"
        c.send()
        #expect(c.steers == 1)
        #expect(c.session.outgoing.list.first?.delivery == .steered)
    }

    @Test("a terminal-only command is refused with a notice and the draft kept")
    func unsupported() {
        let c = chat([])
        c.draft.text = "/clear"
        c.send()
        #expect(c.notice != nil)
        #expect(c.draft.text == "/clear")
        #expect(c.session.outgoing.list.isEmpty)
    }

    @Test("a draft is kept per agent across stores")
    func drafts() {
        let d = scratchDefaults()
        chat([], defaults: d).draft.text = "half a thought"
        #expect(chat([], defaults: d).draft.text == "half a thought")
        #expect(ChatStore(agentId: "a2", defaults: d).draft.isEmpty)
    }

    @Test("picking a command the phone runs opens its menu and clears the draft")
    func pickLocal() {
        let c = chat([])
        c.draft.text = "/mod"
        c.pick(0, cursor: 4)
        #expect(c.draft.text.isEmpty)
        #expect(c.settingsMenu == .model)
    }

    @Test("picking a command mid-draft puts it in for the word typed")
    func pickMidDraft() {
        let c = chat([.agentCommands(.init(agentId: "a1", commands: [Command(name: "review", description: "Review a PR")]))])
        c.draft.text = "please /rev"
        c.pick(0, cursor: 11)
        #expect(c.draft.text == "please /review ")
    }

    @Test("picking a file puts its path in for the mention")
    func pickFile() {
        let c = chat([])
        c.suggest.filesListed("/w", ["src/lanes.rs", "README.md"])
        c.draft.text = "see @lan"
        c.pick(0, cursor: 8)
        #expect(c.draft.text == "see @src/lanes.rs ")
    }
}
