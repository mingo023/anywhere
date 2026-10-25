import Testing
import Wire
@testable import ChatModel

private func session(_ turns: [Turn]) -> Session {
    var s = Session()
    s.show(Transcript(turns: turns))
    return s
}

private func turn(_ end: End?, blocks: [Block] = []) -> Turn {
    Turn(id: "u1", prompt: Message(id: "u1", at: 0, text: "Fix it"), blocks: blocks, end: end)
}

private func agent(_ status: String) -> AgentSummary {
    AgentSummary(
        id: "a1", terminalId: "t1", title: "", cwd: "/", provider: "claude", status: AgentStatus(rawValue: status), attached: true,
        epoch: 1, maxSeq: 0, createdAt: 0, updatedAt: 0)
}

private func ask(_ questions: [Question]) -> Ask {
    Ask(requestId: "r", tool: .shell(command: "ls"), toolName: "Bash", questions: questions)
}

@Test("a send right after a stop goes out even if status still says working")
func aSendRightAfterAStop() {
    let stopped = session([turn(.failed(error: "Interrupted", durationMs: 0))])
    #expect(!Composer.busy(agent("working"), stopped))
    #expect(Composer.busy(agent("idle"), session([turn(nil)])))
    #expect(Composer.busy(agent("working"), nil))
}

@Test("sub agents left in the background leave the composer free")
func subAgentsLeftInTheBackground() {
    let running = Block.tool(Call(id: "a", name: "Task", kind: .agent(SubAgent(description: "")), status: .running))
    let ended = session([turn(.done(durationMs: 5, usage: nil), blocks: [running])])
    let busy = Composer.busy(agent("idle"), ended)
    #expect(!busy)
    #expect(Composer.placeholder(working: busy, stopping: false, canQueue: true) == Composer.placeholder)
    #expect(Composer.hint(.prompt, working: busy, stopping: false, empty: true, canQueue: true) == nil)
    #expect(action(working: busy, stopping: false, empty: true) == .send)
}

@Test("a chat with no turns yet goes by status")
func aChatWithNoTurnsYet() {
    #expect(Composer.busy(agent("working"), Session()))
    #expect(!Composer.busy(agent("idle"), Session()))
}

@Test("a queued send waits while the agent asks and its transcript has no turns")
func aQueuedSendWaitsWhileTheAgentAsks() {
    var s = Session()
    _ = s.outgoing.push("Next", working: true, now: 0)
    s.outgoing.settle(s.transcript, working: Composer.working(agent("needsYou")), now: 1000)
    let released = s.outgoing.released()
    #expect(released.isEmpty)
    #expect(Composer.busy(agent("needsYou"), s))
}

@Test("a send on its way holds the next one")
func aSendOnItsWayHoldsTheNext() {
    var s = session([turn(.done(durationMs: 5, usage: nil))])
    _ = s.outgoing.push("Next", working: false, now: 0)
    #expect(Composer.busy(agent("idle"), s))
    s.show(Transcript())
    #expect(Composer.busy(agent("idle"), s))
}

@Test("the footer hints at what enter does now")
func theFooterHints() {
    #expect(Composer.hint(.prompt, working: false, stopping: false, empty: true, canQueue: true) == nil)
    #expect(Composer.hint(.prompt, working: true, stopping: false, empty: true, canQueue: true) == "Working · esc to stop")
    #expect(Composer.hint(.prompt, working: true, stopping: false, empty: false, canQueue: true) == "⏎ send into this turn · ⌘⏎ queue for after")
    #expect(Composer.hint(.prompt, working: true, stopping: false, empty: false, canQueue: false) == "Working · esc to stop")
    #expect(Composer.hint(.permission(answerable: true), working: true, stopping: false, empty: true, canQueue: true) == "Answer the request above to continue")
    #expect(Composer.hint(.viewOnly, working: false, stopping: false, empty: true, canQueue: true) == nil)
}

@Test("a turn being stopped reads stopping and takes sends only into the queue")
func aTurnBeingStoppedReadsStopping() {
    #expect(Composer.hint(.prompt, working: true, stopping: true, empty: true, canQueue: true) == "Stopping…")
    #expect(Composer.hint(.prompt, working: true, stopping: true, empty: false, canQueue: true) == "Stopping…")
    #expect(Composer.hint(.prompt, working: false, stopping: true, empty: true, canQueue: true) == nil)
    #expect(Composer.placeholder(working: true, stopping: true, canQueue: true) == "Queue a follow-up…")
    #expect(Composer.placeholder(working: true, stopping: false, canQueue: true) == "Send a follow-up into this turn…")
    #expect(Composer.stopBesideSteer(.steer, interrupt: true))
    #expect(!Composer.stopBesideSteer(.queue, interrupt: true))
    #expect(!Composer.stopBesideSteer(.steer, interrupt: false))
}

@Test("a terminal only command points at the terminal only when the agent has its tui")
func aTerminalOnlyCommand() {
    #expect(Composer.terminalOnlySaid("/cost", provider: "claude", chatOnly: false) == "/cost runs only in Claude's terminal. Run it there.")
    #expect(Composer.terminalOnlySaid("/cost", provider: "claude", chatOnly: true) == "/cost isn't available in the chat.")
}

@Test("an observe only agent gets no composer")
func anObserveOnlyAgentGetsNoComposer() {
    #expect(Composer.mode(observeOnly: true, canApprove: false, ask: nil) == .viewOnly)
    #expect(Composer.mode(observeOnly: false, canApprove: true, ask: nil) == .prompt)
    #expect(Composer.mode(observeOnly: true, canApprove: false, ask: ask([])) == .permission(answerable: false))
    #expect(Composer.mode(observeOnly: false, canApprove: true, ask: ask([Question(question: "")])) == .question(answerable: true))
}

@Test("an ask is answerable by the approve scope, not the drive scope")
func anAskIsAnswerableByTheApproveScope() {
    #expect(Composer.mode(observeOnly: true, canApprove: true, ask: ask([])) == .permission(answerable: true))
    #expect(Composer.mode(observeOnly: false, canApprove: false, ask: ask([])) == .permission(answerable: false))
    #expect(Composer.mode(observeOnly: true, canApprove: true, ask: nil) == .viewOnly)
}

@Test("the input grows to eleven rows before it scrolls")
func theInputGrowsToElevenRows() {
    #expect(Composer.maxRows == 11)
}
