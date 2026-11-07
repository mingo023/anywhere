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
