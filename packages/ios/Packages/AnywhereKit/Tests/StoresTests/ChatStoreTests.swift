import ChatModel
import Client
import Foundation
import Fixtures
import Loopback
import Testing
import Wire
@testable import Stores

private func agent(_ status: AgentStatus = .idle, resets: Int? = nil) -> AgentSummary {
    AgentSummary(id: "a1", terminalId: "t-a1", title: "a1", cwd: "/w", provider: "claude", status: status, attached: true,
                 epoch: 1, maxSeq: 0, resets: resets, createdAt: 0, updatedAt: 0)
}

/// A chat the phone drives, of `provider`.
@MainActor
private func chat(of provider: String, connection: DaemonConnection? = nil) -> ChatStore {
    let chat = ChatStore(agentId: "a1", connection: connection)
    let agent = AgentSummary(id: "a1", terminalId: "t-a1", title: "a1", cwd: "/w", provider: provider, status: .idle, attached: true,
                             epoch: 1, maxSeq: 0, createdAt: 0, updatedAt: 0, chatOnly: true)
    chat.seed(agent, requests: [], hello: Sample.hello)
    return chat
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

    @Test("a saved draft keeps small thumbnails and leaves out one too large to be one")
    func draftsKeepSmallThumbnails() {
        let d = scratchDefaults()
        let c = chat([], defaults: d)
        c.draft.images = [
            .init(id: "x", name: "a.png", path: "/home/attachments/a.png", thumbnail: Data(count: 1 << 20)),
            .init(id: "y", name: "b.png", path: "/home/attachments/b.png", thumbnail: Data(count: 4096)),
        ]
        let saved = chat([], defaults: d).draft.images
        #expect(saved.map(\.path) == ["/home/attachments/a.png", "/home/attachments/b.png"])
        #expect(saved.map(\.thumbnail?.count) == [nil, 4096])
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

    @Test("text sent from elsewhere goes out like a typed send and leaves the draft alone")
    func sendText() {
        let c = chat([])
        c.draft.text = "half a thought"
        #expect(c.send(text: "Fix what holds back PR #12"))
        #expect(c.draft.text == "half a thought")
        #expect(c.sends == 1)
        #expect(c.session.outgoing.list.map(\.text) == ["Fix what holds back PR #12"])
        let working = chat([.agentUpdate(.init(agent: agent(.working)))])
        #expect(working.send(text: "also this"))
        #expect(working.session.outgoing.list.first?.delivery == .steered)
    }

    @Test("text sent from elsewhere is refused by a phone that may only watch")
    func sendTextObserveOnly() {
        let c = ChatStore(agentId: "a1")
        var hello = Sample.hello
        hello.scopes = [.observe]
        c.seed(agent(), requests: [], hello: hello)
        #expect(!c.send(text: "Fix it"))
        #expect(c.session.outgoing.list.isEmpty)
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

    @Test("a bare /model or /effort with nothing for the phone to list says to name one instead of opening an empty menu")
    func aBareSettingWithNothingToListSaysToNameOne() {
        let c = chat(of: "opencode")
        _ = c.apply(.agentProviders(Sample.providers))
        for command in ["/model", "/effort"] {
            c.draft.text = command
            c.send()
            #expect(c.settingsMenu == nil, "\(command)")
            #expect(c.notice == "There's no list of OpenCode's choices here. Send \(command) with one.")
        }
        let pi = chat(of: "pi")
        _ = pi.apply(.agentProviders(Sample.providers))
        pi.draft.text = "/thinking"
        pi.send()
        #expect(pi.settingsMenu == .effort, "Pi lists its levels")
    }

    @Test("picking /mcp leaves the caret after the tag it puts in")
    func pickMcp() {
        let c = chat([])
        c.draft.text = "/mc"
        let caret = c.pick(0, cursor: 3)
        #expect(c.draft.text == "@mcp/")
        #expect(caret == 5)
    }

    @Test("picking /new asks for New Session and keeps the draft, as typing it does")
    func pickNew() {
        var a = agent()
        a.provider = "codex"
        let c = ChatStore(agentId: "a1")
        c.seed(a, requests: [], hello: Sample.hello)
        c.draft.text = "/ne"
        c.pick(0, cursor: 3)
        #expect(c.newSession != nil)
        #expect(c.draft.text == "/ne")
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

    @Test("files and lines handed over from Files land once, all of them, in the chat they were for")
    func additionsFromFiles() {
        let files = FilesStore()
        let c = ChatStore(agentId: "a1", files: files)
        c.seed(agent(), requests: [], hello: Sample.hello)
        c.draft.text = "look at"
        files.mention(tree: "/elsewhere", path: "b.rs")
        files.mention(tree: "/w", path: "c.rs", for: "a2")
        c.appear()
        c.disappear()
        #expect(c.draft.text == "look at")
        files.mention(tree: "/w", path: "src/a.rs", for: "a1")
        files.mention(tree: "/w", path: "src/b.rs", for: "a1")
        files.quote("src/a.rs:2\n```rust\nlet x = 1;\n```", tree: "/w", for: "a1")
        c.appear()
        c.disappear()
        c.appear()
        c.disappear()
        #expect(c.draft.text == "look at @src/a.rs @src/b.rs \n\nsrc/a.rs:2\n```rust\nlet x = 1;\n```\n")
        #expect(files.takeAdditions(for: "a2", in: "/w") == ["@c.rs"])
    }

    @Test("a chat stays viewed while a sub-agent's screen opens over it and after it closes")
    func viewedUnderASubAgent() {
        let c = chat([])
        c.appear()
        c.appear()
        c.disappear()
        #expect(c.viewing)
        c.disappear()
        #expect(!c.viewing)
        c.disappear()
        c.appear()
        #expect(c.viewing)
    }

    @Test("leaving a chat tells a Mac that reads front.v1 the app is still in front, and one that doesn't nothing, as it would read no chats as the app gone")
    func leavingTellsFrontOnlyToAMacThatReadsIt() async throws {
        let server = try await TestServer { peer, frame in
            if case .hello = frame.message { peer.send(ServerFrame(id: frame.id, .helloOk(Sample.hello))) }
        }
        defer { server.stop() }
        let connection = DaemonConnection(config: .init(host: server.host, token: "t", clientId: "c"))
        await connection.start()
        defer { Task { await connection.stop() } }
        for _ in 0..<200 where await !connection.isOnline { try await Task.sleep(for: .milliseconds(5)) }
        var old = Sample.hello
        old.caps = old.caps?.filter { $0 != Caps.front }
        for hello in [old, Sample.hello] {
            let c = ChatStore(agentId: "a1", connection: connection)
            c.seed(agent(), requests: [], hello: hello)
            c.disappear()
        }
        let views = { server.received.compactMap { if case .agentView(let v) = $0.message { v } else { nil } } }
        for _ in 0..<200 where views().isEmpty { try await Task.sleep(for: .milliseconds(5)) }
        try await Task.sleep(for: .milliseconds(50))
        #expect(views() == [.init(agentIds: [], front: true)])
    }

    @Test("a sub-agent's screen waits for the first page and pages back to find a sub-agent not held")
    func aSubAgentIsSoughtInOlderPages() {
        let unloaded = chat([])
        #expect(!unloaded.whole)
        let latest = chat([timeline(Sample.chat("background-agents"), hasOlder: true)])
        #expect(!latest.whole)
        latest.seek("tu-a1")
        #expect(!latest.loadingOlder)
        latest.seek("tu-none")
        #expect(latest.loadingOlder)
        let all = chat([timeline(Sample.chat("background-agents"))])
        all.seek("tu-none")
        #expect(all.whole && !all.loadingOlder)
        _ = all.apply(stream(item("late", 10_000)))
        #expect(!all.whole, "a refold is due")
    }

    @Test("a sub-agent's screen asks the chat to show the main agent's call that spawned it")
    func aSubAgentIsRevealedAtItsCall() {
        let c = chat([timeline(Sample.chat("background-agents"))])
        c.reveal("tu-a1")
        #expect(c.revealing == "tu-a1")
        c.revealed()
        #expect(c.revealing == nil)
    }

    @Test("the background tray and the agent map are worked out as the transcript refolds")
    func backgroundAndAgentsFollowTheTranscript() {
        let c = chat([timeline(Sample.chat("background-agents"))])
        #expect(!c.background.isEmpty)
        #expect(!c.agents.isEmpty)
        #expect(c.background == inBackground(c.session.transcript.turns[...]).map(\.call))
        #expect(c.agents == spawned(c.session.transcript))
        _ = c.apply(timeline([]))
        _ = c.apply(.agentUpdate(.init(agent: agent(resets: 1))))
        c.flush()
        #expect(c.background.isEmpty)
        #expect(c.agents.isEmpty)
    }

    @Test("an offline send keeps the draft and sends nothing, and Stop waits too")
    func offlineKeepsTheDraft() {
        let c = chat([.agentUpdate(.init(agent: agent(.working)))])
        c.follow(online: false, mac: "Studio")
        c.draft.text = "also the tests"
        c.send()
        #expect(c.draft.text == "also the tests")
        #expect(c.session.outgoing.list.isEmpty)
        c.draft.text = ""
        c.press()
        #expect(!c.stopping)
    }

    @Test("a setting the Mac refuses says why in the notice, and /compact with instructions says an agent's own compact takes none")
    func refusalsAreSaid() async throws {
        let refusal = "Pi runs every tool without asking, so its access is always full"
        let server = try await TestServer { peer, frame in
            switch frame.message {
            case .hello: peer.send(ServerFrame(id: frame.id, .helloOk(Sample.hello)))
            case .agentSettings: peer.send(ServerFrame(id: frame.id, .error(ServerError(message: refusal))))
            default: break
            }
        }
        defer { server.stop() }
        let connection = DaemonConnection(config: .init(host: server.host, token: "t", clientId: "c"))
        await connection.start()
        defer { Task { await connection.stop() } }
        for _ in 0..<200 where await !connection.isOnline { try await Task.sleep(for: .milliseconds(5)) }
        let c = chat(of: "pi", connection: connection)
        let compacts = { server.received.filter { if case .agentCompact = $0.message { true } else { false } }.count }

        c.settings(access: "ask")
        for _ in 0..<200 where c.notice == nil { try await Task.sleep(for: .milliseconds(5)) }
        #expect(c.notice == refusal)

        c.draft.text = "/compact keep the plan"
        c.send()
        #expect(c.notice == "Pi's /compact takes no instructions. Send /compact alone.")
        c.draft.text = "/compact"
        c.send()
        #expect(c.notice == nil)
        for _ in 0..<200 where compacts() == 0 { try await Task.sleep(for: .milliseconds(5)) }
        try await Task.sleep(for: .milliseconds(50))
        #expect(compacts() == 1, "only the bare /compact compacted")
    }

    @Test("an ACP agent's /compact compacts once the Mac says it can, and never reaches the model before")
    func anAcpAgentsCompactCompactsOnceTheMacSaysItCan() async throws {
        let server = try await TestServer { peer, frame in
            if case .hello = frame.message { peer.send(ServerFrame(id: frame.id, .helloOk(Sample.hello))) }
        }
        defer { server.stop() }
        let connection = DaemonConnection(config: .init(host: server.host, token: "t", clientId: "c"))
        await connection.start()
        defer { Task { await connection.stop() } }
        for _ in 0..<200 where await !connection.isOnline { try await Task.sleep(for: .milliseconds(5)) }
        let sent = {
            server.received.compactMap {
                switch $0.message {
                case .agentCompact: "compact"
                case .agentPrompt(let p): p.text
                default: nil
                }
            }
        }
        let c = chat(of: "opencode", connection: connection)
        c.draft.text = "/compact"
        c.send()
        #expect(c.notice != nil, "OpenCode's own /compact is its TUI's")

        _ = c.apply(.agentProviders(Sample.providers))
        c.draft.text = "/compact"
        c.send()
        for _ in 0..<200 where sent().isEmpty { try await Task.sleep(for: .milliseconds(5)) }
        try await Task.sleep(for: .milliseconds(50))
        #expect(sent() == ["compact"])
    }

    @Test("the access pill is a menu in a chat the phone drives, but not for an agent that never asks and so has only Full")
    func anAgentThatNeverAsksHasNoAccessToPick() {
        let pi = chat(of: "pi")
        let opencode = chat(of: "opencode")
        #expect(pi.setsAccess && opencode.setsAccess, "until the Mac says which agents never ask")
        _ = pi.apply(.agentProviders(Sample.providers))
        _ = opencode.apply(.agentProviders(Sample.providers))
        #expect(!pi.setsAccess && opencode.setsAccess)
        #expect(!chat([]).setsAccess, "an agent in its terminal sets its own")
    }

    @Test("the notice clears once the Mac is back")
    func noticeClearsOnline() {
        let c = chat([])
        c.draft.text = "/clear"
        c.send()
        #expect(c.notice != nil)
        c.connected(Sample.hello)
        #expect(c.notice == nil)
        #expect(c.online)
    }

    @Test("a failed request names the Mac in words, not the error's name")
    func failuresNameTheMac() async {
        let c = chat([.permissionRequest(.init(request: Sample.chatQuestion(agentId: "a1")))])
        c.follow(online: true, mac: "Studio")
        c.skipQuestions()
        for _ in 0..<5 { await Task.yield() }
        #expect(c.notice == "Not connected to Studio")
    }

    @Test("a send while an image uploads arms, keeps the draft, and goes once the image lands")
    func armedSendGoesOnLanding() {
        let c = chat([])
        c.draft.images = [.init(id: "x", name: "a.png")]
        c.draft.text = "see this"
        c.send()
        #expect(c.armed)
        #expect(c.draft.text == "see this")
        #expect(c.session.outgoing.list.isEmpty)
        c.landed("x", "/att/a.png")
        #expect(!c.armed)
        #expect(c.draft.isEmpty)
        #expect(c.session.outgoing.list.map(\.text) == [prompt("see this", images: ["/att/a.png"])])
    }

    @Test("an armed send disarms when an image fails, which says why and blocks sending until retried or removed")
    func failedImageDisarms() {
        let c = chat([])
        c.draft.images = [.init(id: "x", name: "a.png"), .init(id: "y", name: "b.png")]
        c.draft.text = "see this"
        c.send()
        c.failed("x", AttachmentError.tooLarge)
        #expect(!c.armed)
        #expect(c.draft.images.first?.reason == "Images over 20 MB can't be attached")
        c.landed("y", "/att/b.png")
        c.send()
        #expect(c.session.outgoing.list.isEmpty)
        c.detach("x")
        c.send()
        #expect(c.session.outgoing.list.count == 1)
    }

    @Test("pressing an armed send disarms it")
    func pressDisarms() {
        let c = chat([])
        c.draft.images = [.init(id: "x", name: "a.png")]
        c.draft.text = "see this"
        c.send()
        c.press()
        #expect(!c.armed)
        c.landed("x", "/att/a.png")
        #expect(c.draft.text == "see this")
    }

    @Test("an image with no Mac to take it fails with the reason, and goes again on retry")
    func imageRetry() {
        let c = chat([])
        c.attach(Data([1, 2, 3]), name: "a.png")
        #expect(c.draft.images.first?.failed == true)
        #expect(c.draft.images.first?.reason == "Not connected to Mingo's MacBook Pro")
        c.follow(online: true, mac: "Studio")
        c.retryUpload(c.draft.images[0].id)
        #expect(c.draft.images.first?.reason == "Not connected to Studio")
    }

    @Test("retrying an undelivered send brings back its images as landed tiles")
    func retryKeepsImages() async {
        let c = chat([])
        c.draft.images = [.init(id: "x", name: "a.png", thumbnail: Data([7]))]
        c.landed("x", "/att/a.png")
        c.draft.text = "see this"
        c.send()
        for _ in 0..<5 { await Task.yield() }
        let failed = c.session.outgoing.list.first
        #expect(failed?.delivery == .failed)
        c.retry(failed!.id)
        #expect(c.draft.text == "see this")
        #expect(c.draft.images.map(\.path) == ["/att/a.png"])
        #expect(c.draft.images.first?.thumbnail == Data([7]))
    }

    @Test("editing a queued send puts its images back as tiles, not paths in the text")
    func editKeepsImages() {
        let c = chat([.agentUpdate(.init(agent: agent(.working)))])
        c.draft.images = [.init(id: "x", name: "a.png", path: "/att/a.png")]
        c.draft.text = "after this"
        c.send(queue: true)
        let queued = c.session.outgoing.list.first!
        c.editQueued(queued.id)
        #expect(c.draft.text == "after this")
        #expect(c.draft.images.map(\.path) == ["/att/a.png"])
    }

    @Test("/new asks for New Session where the chat's agent runs, with what follows as its prompt, and keeps the draft")
    func newOpensNewSession() {
        var a = agent()
        a.cwd = "/w/.worktrees/calm-otter/src"
        let c = ChatStore(agentId: "a1")
        c.seed(a, requests: [], hello: Sample.hello)
        c.draft.text = "/new fix the build"
        c.send()
        #expect(c.newSession == .init(cwd: "/w/.worktrees/calm-otter/src", prompt: "fix the build"))
        #expect(c.draft.text == "/new fix the build")
        c.openedNewSession()
        #expect(c.newSession == nil)
    }

    @Test("the draft that asked for a New Session goes once a session with its prompt starts, unless it changed since")
    func newSessionStartClearsDraft() {
        let c = ChatStore(agentId: "a1")
        c.seed(agent(), requests: [], hello: Sample.hello)
        c.draft.text = "/new fix the build"
        c.startNewSession("fix the build")
        c.openedNewSession()
        c.started(prompt: "something else")
        #expect(c.draft.text == "/new fix the build")
        c.started(prompt: "fix the build")
        #expect(c.draft.isEmpty)

        c.draft.text = "/new"
        c.startNewSession("")
        c.openedNewSession()
        c.draft.text = "/new, then this"
        c.started(prompt: nil)
        #expect(c.draft.text == "/new, then this")
    }

    @Test("a latest page that starts past what's held drops it and is taken as the first page")
    func gapTakesAFreshPage() {
        let c = chat([timeline([item("i1", 1), item("i2", 2), item("i3", 3)])])
        c.page(.init(agentId: "a1", epoch: 1, items: [item("i600", 600), item("i601", 601)], hasOlder: true, maxSeq: 601), older: false, after: 3)
        c.flush()
        #expect(c.items.map(\.seq) == [600, 601])
        #expect(c.index == ["i600": 0, "i601": 1])
        #expect(c.hasOlder)
    }

    @Test("a latest page that reaches back to what's held adds to it")
    func overlapKeepsWhatsHeld() {
        let c = chat([timeline([item("i1", 1), item("i2", 2), item("i3", 3)])])
        c.page(.init(agentId: "a1", epoch: 1, items: [item("i3", 3), item("i4", 4)], hasOlder: true, maxSeq: 4), older: false, after: 3)
        #expect(c.items.map(\.seq) == [1, 2, 3, 4])
        #expect(!c.hasOlder)
    }

    @Test("Stop right after a send is ignored, and works a second later")
    func stopGuard() {
        let clock = Clock()
        let c = ChatStore(agentId: "a1", clock: { clock.now })
        c.seed(agent(.working), requests: [], hello: Sample.hello)
        c.flush()
        c.draft.text = "also the tests"
        c.send()
        #expect(c.action == .stop)
        clock.now += 300
        c.press()
        #expect(!c.stopping)
        clock.now += 1000
        c.press()
        #expect(c.stopping)
    }

    @Test("an answer goes once, and its buttons come back if it fails")
    func answersGoOnce() async {
        let ask = PermissionRequest(requestId: "r1", agentId: "a1", toolName: "Bash", detail: .shell(command: "ls", description: nil))
        let c = chat([.permissionRequest(.init(request: ask))])
        c.allow()
        #expect(c.answering == "r1")
        for _ in 0..<5 { await Task.yield() }
        #expect(c.answering == nil)
        #expect(c.notice != nil)
        c.follow(online: false, mac: "Studio")
        c.allow()
        #expect(c.answering == nil)
    }

    @Test("a session the Mac no longer lists closes its composer until it's listed again")
    func closedSession() {
        let c = chat([])
        #expect(c.apply(.agentList(.init(agents: []))))
        #expect(c.mode == .closed)
        c.draft.text = "anyone there?"
        c.send()
        #expect(c.session.outgoing.list.isEmpty)
        #expect(c.apply(.agentList(.init(agents: [agent()]))))
        #expect(c.mode == .prompt)
    }

    @Test("a session pocketd reports closed closes its composer")
    func closedByUpdate() {
        let c = chat([])
        #expect(c.apply(.agentUpdate(.init(agent: agent(.closed)))))
        #expect(c.mode == .closed)
        #expect(c.apply(.agentList(.init(agents: [agent(.closed)]))) == false)
        #expect(c.mode == .closed)
    }
}

@MainActor
private final class Clock {
    var now = 0
}
