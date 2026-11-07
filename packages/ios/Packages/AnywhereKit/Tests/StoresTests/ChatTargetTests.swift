import Domain
import Testing
import Wire
@testable import Stores

@Suite("Handing files or lines to a chat")
struct ChatTargetTests {
    private func agent(_ id: String, cwd: String = "/w", status: AgentStatus = .idle, updatedAt: Int) -> AgentSummary {
        AgentSummary(
            id: id, terminalId: "t-\(id)", title: "Session \(id)", cwd: cwd, provider: "claude", status: status, attached: true, epoch: 1, maxSeq: 0,
            createdAt: 0, updatedAt: updatedAt)
    }

    @Test("a tree's sessions are those in it, most recent first")
    func sessions() {
        let found = ChatSession.on("/w", trees: ["/w"], [agent("a1", updatedAt: 1), agent("a2", cwd: "/w/src", updatedAt: 3), agent("a3", cwd: "/other", updatedAt: 9)])
        #expect(found.map(\.id) == ["a2", "a1"])
        #expect(found.first?.agent == "Claude")
    }

    @Test("a worktree inside the main tree keeps its sessions, and the main tree doesn't take them")
    func nested() {
        let trees = ["/repo", "/repo/.claude/worktrees/x", "/repo-two"]
        let agents = [agent("main", cwd: "/repo/src", updatedAt: 1), agent("nested", cwd: "/repo/.claude/worktrees/x/src", updatedAt: 2),
                      agent("sibling", cwd: "/repo-two", updatedAt: 3)]
        #expect(ChatSession.on("/repo", trees: trees, agents).map(\.id) == ["main"])
        #expect(ChatSession.on("/repo/.claude/worktrees/x", trees: trees, agents).map(\.id) == ["nested"])
    }

    @Test("a tree the Mac hasn't listed yet still has the sessions in it")
    func unlisted() {
        #expect(ChatSession.on("/w", trees: [], [agent("a1", cwd: "/w/src", updatedAt: 1), agent("a2", cwd: "/w-two", updatedAt: 2)]).map(\.id) == ["a1"])
    }

    @Test("the chat that opened the screen takes it, else the only session, else the reader picks, and none without a session")
    func target() {
        let a = ChatSession(id: "a1", title: "Fix the build", agent: "Claude")
        let b = ChatSession(id: "a2", title: "Docs", agent: "Codex")
        #expect(ChatTarget([a, b], opener: "a2") == .one(b))
        #expect(ChatTarget([a], opener: nil) == .one(a))
        #expect(ChatTarget([a, b], opener: nil) == .choose([a, b]))
        #expect(ChatTarget([a, b], opener: "elsewhere") == .choose([a, b]))
        #expect(ChatTarget([], opener: "a1") == .none)
    }

    @Test("a session waiting on a permission or question is passed over for one that can take a prompt")
    func ready() {
        let waiting = ChatSession.on("/w", trees: [], [agent("a1", status: .needsYou, updatedAt: 2), agent("a2", status: .working, updatedAt: 1)])
        #expect(waiting.map(\.ready) == [false, true])
        #expect(ChatTarget(waiting, opener: "a1") == .one(waiting[1]))
        let c = ChatSession(id: "a3", title: "Docs", agent: "Codex")
        #expect(ChatTarget(waiting + [c], opener: nil) == .choose([waiting[1], c]))
    }

    @Test("with none that can take a prompt, the waiting ones are still the target")
    func noneReady() {
        let a = ChatSession(id: "a1", title: "Fix the build", agent: "Claude", ready: false)
        #expect(ChatTarget([a], opener: nil) == .one(a))
    }

    @Test("a handoff goes to the target session, waits on the reader's pick, or starts a session with its prompt")
    func handoff() {
        let a = ChatSession(id: "a1", title: "Fix the build", agent: "Claude")
        let b = ChatSession(id: "a2", title: "Docs", agent: "Codex")
        #expect(ChatHandoff(.one(a), mac: "m", project: "/p", tree: "/w", prompt: "why?") == .send(a))
        #expect(ChatHandoff(.choose([a, b]), mac: "m", project: "/p", tree: "/w", prompt: nil) == .choose([a, b]))
        #expect(ChatHandoff(.none, mac: "m", project: "/p", tree: "/w", prompt: "why?") == .start(.newSession(mac: "m", project: "/p", place: "/w", prompt: "why?")))
        #expect(ChatHandoff(.none, mac: "m", project: "/p", tree: "/w", prompt: nil) == .start(nil))
        #expect(ChatHandoff.added(to: a) == "Added to Claude's input")
    }
}
