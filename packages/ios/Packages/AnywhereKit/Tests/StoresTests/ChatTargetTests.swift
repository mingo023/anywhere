import Testing
import Wire
@testable import Stores

@Suite("Handing files or lines to a chat")
struct ChatTargetTests {
    private func agent(_ id: String, cwd: String = "/w", worktree: String? = nil, updatedAt: Int) -> AgentSummary {
        AgentSummary(
            id: id, terminalId: "t-\(id)", title: "Session \(id)", cwd: cwd, provider: "claude", status: .idle, attached: true, epoch: 1, maxSeq: 0,
            createdAt: 0, updatedAt: updatedAt, worktree: worktree)
    }

    @Test("a tree's sessions are those in it or its worktree, most recent first")
    func sessions() {
        let found = ChatSession.on("/w", [
            agent("a1", updatedAt: 1), agent("a2", cwd: "/w/src", worktree: "/w", updatedAt: 3), agent("a3", cwd: "/other", updatedAt: 9),
        ])
        #expect(found.map(\.id) == ["a2", "a1"])
        #expect(found.first?.agent == "Claude")
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
}
