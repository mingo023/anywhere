import Testing
import Wire
@testable import ChatModel

private func call(_ kind: ToolKind, _ at: Int) -> Call {
    Call(id: "c\(at)", name: "Task", kind: kind, status: .running, at: at)
}

private func agent(_ task: String, _ calls: [Call]) -> Call {
    let turn = Turn(id: "t", blocks: calls.map(Block.tool))
    return call(.agent(SubAgent(description: task, transcript: Transcript(turns: [turn]))), 0)
}

private func summary(provider: String = "claude", model: String? = nil, effort: String? = nil) -> AgentSummary {
    AgentSummary(
        id: "a", terminalId: "t", title: "", cwd: "/", provider: provider, model: model, effort: effort, status: .idle, attached: true,
        epoch: 1, maxSeq: 0, createdAt: 0, updatedAt: 0)
}

@Test("a task says what it does now and a command just its name")
func aTaskSaysWhatItDoes() {
    let reading = agent("Review the spec", [call(.search(query: "fold"), 1), call(.read(path: "docs/spec.md"), 2)])
    #expect(Background.name(reading) == "Review the spec" && Background.doing(reading) == "Read docs/spec.md")
    #expect(Background.doing(agent("Review the spec", [])) == "Starting…")
    let server = call(.shell(command: "bun run dev\n"), 0)
    #expect(Background.name(server) == "bun run dev" && Background.doing(server) == nil)
}

@Test("the folded tray shows the newest step of any task")
func theFoldedTrayShowsTheNewestStep() {
    let a = agent("a", [call(.read(path: "old.rs"), 5)])
    let b = agent("b", [call(.search(query: "left-pad"), 9)])
    let server = call(.shell(command: "bun run dev"), 20)
    #expect(Background.newest([a, b, server]) == "Find left-pad")
    #expect(Background.newest([server]) == nil)
}

@Test("a chat left with only background work still offers a stop")
func aChatLeftWithOnlyBackgroundWork() {
    let (a, server) = (agent("Review", []), call(.shell(command: "bun run dev"), 0))
    #expect(Background.stops(chatOnly: true, interrupt: true, busy: false, [a, server]))
    #expect(!Background.stops(chatOnly: false, interrupt: true, busy: false, [a]))
    #expect(!Background.stops(chatOnly: true, interrupt: false, busy: false, [a]))
}

@Test("the tray offers no stop that would end a turn or reach no sub agent")
func theTrayOffersNoStop() {
    let (a, server) = (agent("Review", []), call(.shell(command: "bun run dev"), 0))
    #expect(!Background.stops(chatOnly: true, interrupt: true, busy: true, [a]))
    #expect(!Background.stops(chatOnly: true, interrupt: true, busy: false, [server]))
}

@Test("the queue lists only queued sends")
func theQueueListsOnlyQueuedSends() {
    var out = Outgoing()
    _ = out.push("lost", working: false, now: 0)
    out.settle(Transcript(), working: false, now: 200_000)
    _ = out.push("next", working: true, now: 200_000)
    _ = out.push("sending", working: false, now: 200_000)
    #expect(Queue.waiting(out.list).map(\.text) == ["next"])
}

@Test("editing a held send keeps what was typed")
func editingAHeldSend() {
    #expect(Queue.editing("", "held") == "held")
    #expect(Queue.editing("  ", "held") == "held")
    #expect(Queue.editing("typed", "held") == "typed\n\nheld")
}

@Test("a stuck codex goal reads as stuck")
func aStuckCodexGoal() {
    #expect(GoalStrip.statusLabel("blocked") == "Stuck")
    #expect(GoalStrip.statusLabel("failed") == "Gave up")
}

@Test("a goal shows its tokens against a budget only when it has one")
func aGoalShowsItsTokens() {
    let goal = { (used: Int, budget: Int) in Goal(objective: "", status: "active", tokensUsed: used, tokenBudget: budget) }
    #expect(GoalStrip.spent(goal(16510, 50000)) == "16,510 / 50,000 tokens")
    #expect(GoalStrip.spent(goal(16510, 0)) == "16,510 tokens")
    #expect(GoalStrip.spent(goal(0, 0)) == nil)
}

@Test("thousands groups digits")
func thousandsGroupsDigits() {
    #expect([0, 999, 1000, 184_000, 1_000_000].map(thousands) == ["0", "999", "1,000", "184,000", "1,000,000"])
}

@Test("the model pill reads model and effort")
func theModelPillReads() {
    #expect(ModelPill.label(summary(model: "claude-opus-4-5", effort: "xhigh")) == ("Opus 4.5", "Extra high"))
    #expect(ModelPill.label(summary(model: "claude-opus-4-5", effort: "medium")).effort == "Medium")
    #expect(ModelPill.label(summary(model: "claude-opus-4-5")).effort == nil)
}

@Test("a claude alias is the model its full id names")
func aClaudeAliasIsTheModel() {
    #expect(ModelPill.runs(summary(model: "claude-opus-5-5"), "opus"))
    #expect(ModelPill.runs(summary(model: "opus"), "opus"))
    #expect(!ModelPill.runs(summary(model: "claude-opus-5-5"), "sonnet"))
    #expect(ModelPill.runs(summary(provider: "codex", model: "gpt-6-sol"), "gpt-6-sol"))
    #expect(!ModelPill.runs(summary(provider: "codex", model: "gpt-6-sol"), "sol"))
}

@Test("level is low at 74 warn at 75 and 89 danger at 90")
func levelIsLowAt74() {
    let at = { ContextMeter.level($0, 200_000) }
    #expect([148_000, 149_998, 150_000, 179_998, 180_000, 400_000].map(at) == [.low, .low, .warn, .warn, .danger, .danger])
}

@Test("an empty window is zero percent full")
func anEmptyWindowIsZeroPercentFull() {
    #expect(ContextMeter.percent(184_000, 0) == 0 && ContextMeter.level(184_000, 0) == .low)
}

@Test("context is unknown without a window")
func contextIsUnknownWithoutAWindow() {
    var a = summary()
    a.tokensUsed = 184_000
    #expect(ContextMeter.context(a) == nil)
    a.contextWindow = 0
    #expect(ContextMeter.context(a) == nil)
    a.contextWindow = 200_000
    #expect(ContextMeter.context(a)! == (184_000, 200_000))
    #expect(ContextMeter.lines(184_000, 200_000) == ("184,000 / 200,000 tokens", "16,000 tokens remaining"))
}

private func to(_ drafts: inout Drafts<String>, _ input: inout String, _ agent: String) {
    let held = input
    if let draft = drafts.switchTo(agent, held: { held }) { input = draft }
}

@Test("switching chats keeps each draft")
func switchingChatsKeepsEachDraft() {
    var drafts = Drafts(empty: "")
    var input = ""
    to(&drafts, &input, "a")
    input += "for a"
    to(&drafts, &input, "b")
    #expect(input == "")
    #expect(drafts.parked("a") == "for a")
    input += "for b"
    to(&drafts, &input, "a")
    #expect(input == "for a")
    to(&drafts, &input, "a")
    #expect(input == "for a")
    to(&drafts, &input, "b")
    #expect(input == "for b")
}

@Test("a closed chat drops its draft")
func aClosedChatDropsItsDraft() {
    var drafts = Drafts(empty: "")
    var input = ""
    to(&drafts, &input, "a")
    input += "for a"
    to(&drafts, &input, "b")
    input += "for b"
    let a = drafts.close("a")
    #expect(!a)
    let b = drafts.close("b")
    #expect(b)
    #expect(drafts.owner == nil)
    input = ""
    to(&drafts, &input, "a")
    #expect(input == "")
}
