import Testing
import Wire
@testable import ChatModel

private func agent(_ status: String, failed: Bool = false) -> AgentSummary {
    AgentSummary(
        id: "a1", terminalId: "t1", title: "", cwd: "/", provider: "claude", status: AgentStatus(rawValue: status), failed: failed,
        attached: true, epoch: 1, maxSeq: 0, createdAt: 0, updatedAt: 0)
}

@Test("the chat's bar says the agent needs you or failed, else working, else names the Mac")
func theChatsBarSaysWhereTheAgentStands() {
    #expect(ChatTitle.subtitle(agent("needsYou"), busy: true, mac: "Studio") == "Needs you")
    #expect(ChatTitle.subtitle(agent("done", failed: true), busy: false, mac: "Studio") == "Failed")
    #expect(ChatTitle.subtitle(agent("working"), busy: false, mac: "Studio") == "Working")
    #expect(ChatTitle.subtitle(agent("idle"), busy: true, mac: "Studio") == "Working")
    #expect(ChatTitle.subtitle(agent("idle"), busy: false, mac: "Studio") == "Studio")
    #expect(ChatTitle.subtitle(nil, busy: false, mac: "Studio") == "Studio")
}

@Test("the chat's bar counts a tree's changed lines, else its files, and says nothing of a clean tree")
func theChatsBarCountsChanges() {
    #expect(ChatTitle.change(files: 3, added: 12, removed: 3) == "+12 −3")
    #expect(ChatTitle.change(files: 2, added: 0, removed: 0) == "2 files")
    #expect(ChatTitle.change(files: 1, added: 0, removed: 0) == "1 file")
    #expect(ChatTitle.change(files: 0, added: 0, removed: 0) == nil)
}

@Test("the chat's bar names the branch, or says HEAD is detached")
func theChatsBarNamesTheBranch() {
    #expect(ChatTitle.branch("fix", detached: false) == "fix")
    #expect(ChatTitle.branch("fix", detached: true) == "Detached HEAD")
    #expect(ChatTitle.branch(nil, detached: false) == "No branch")
}
