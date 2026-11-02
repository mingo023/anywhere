import Testing

@testable import ChatModel

private func msg(_ text: String) -> Message { Message(id: "m", at: 0, text: text) }
private func call(_ kind: ToolKind, _ status: CallStatus) -> Block { .tool(Call(id: "t", name: "", kind: kind, status: status)) }
private func shell() -> Block { call(.shell(command: "ls"), .ok) }
private func agent() -> Block { call(.agent(SubAgent(description: "")), .running) }
private func think() -> Block { .thinking(msg("hmm")) }
private func text(_ t: String) -> Block { .text(msg(t)) }
private let done: End? = .done(durationMs: 1, usage: nil)
private func turn(_ id: String, _ blocks: [Block], _ end: End?) -> Turn { Turn(id: id, prompt: msg("go"), blocks: blocks, end: end) }
private func own(_ id: String, _ blocks: [Block], _ end: End?) -> Turn { Turn(id: id, prompt: nil, blocks: blocks, end: end) }
private func one(_ blocks: [Block]) -> Transcript { Transcript(turns: [turn("u1", blocks, done)]) }
private func ids(_ t: Transcript, _ pending: Int = 0) -> [String] { rows(t, pending: pending, sending: pending > 0).map { $0.id(t) } }
private func todos(_ text: String) -> Block { .todos(id: text, todos: [Todo(text: text, status: .pending)]) }

@Test("a reply lays out as fold, phases, then its answer")
func aReplyLaysOutAsFoldPhasesThenItsAnswer() {
    let t = one([think(), shell(), shell(), text("Done.")])
    #expect(rows(t, pending: 0, sending: false) == [.prompt(turn: 0), .fold(turn: 0), .phase(turn: 0, blocks: 0..<3), .text(turn: 0, block: 3, range: 0..<5), .end(turn: 0)])
}

@Test("text between steps stands whole between their phases")
func textBetweenStepsStandsWholeBetweenTheirPhases() {
    let t = one([shell(), text("Now the tests.\n\nThey cover both paths."), think(), shell(), text("All green.")])
    #expect(ids(t) == ["u1", "u1#fold", "u1#p0", "u1#1.0", "u1#1.16", "u1#p2", "u1#4.0", "u1#end"])
    #expect(rows(t, pending: 0, sending: false)[5] == .phase(turn: 0, blocks: 2..<4))
    #expect(narrates(t.turns[0], 1) && !narrates(t.turns[0], 4), "the answer after the last step isn't between steps")
}

@Test("sub-agents are steps of the phase they run in, and the latest todos follow the work")
func subAgentsAreStepsOfThePhaseTheyRunInAndTheLatestTodosFollowTheWork() {
    let t = one([todos("a"), shell(), agent(), shell(), agent(), todos("b"), text("Both reported.")])
    #expect(ids(t) == ["u1", "u1#fold", "u1#p1", "u1#5", "u1#6.0", "u1#end", "u1#live"], "the running sub-agents keep the reply at work")
    #expect(rows(t, pending: 0, sending: false)[2] == .phase(turn: 0, blocks: 1..<5))
    #expect(rows(t, pending: 0, sending: false)[3] == .tasks(turn: 0, block: 5))
}

@Test("narration before a sub-agent stands above its phase")
func narrationBeforeASubAgentStandsAboveItsPhase() {
    let t = one([text("I'll hand the audit to a subagent."), agent(), text("It's on it.")])
    #expect(Array(rows(t, pending: 0, sending: false)[2..<5]) == [.text(turn: 0, block: 0, range: 0..<34), .phase(turn: 0, blocks: 1..<2), .text(turn: 0, block: 2, range: 0..<11)])
}

@Test("a reply stopped before anything came has no fold")
func aReplyStoppedBeforeAnythingCameHasNoFold() {
    #expect(ids(Transcript(turns: [turn("u1", [], .failed(error: "interrupted", durationMs: 0))])) == ["u1", "u1#end"])
    #expect(ids(Transcript(turns: [turn("u1", [], nil)])) == ["u1", "u1#fold", "u1#live"], "an open one keeps it for its live line")
}

@Test("a question ends the phase and stands alone")
func aQuestionEndsThePhaseAndStandsAlone() {
    let t = one([shell(), call(.question([]), .ok), shell()])
    #expect(Array(rows(t, pending: 0, sending: false)[2..<5]) == [.phase(turn: 0, blocks: 0..<1), .question(turn: 0, block: 1), .phase(turn: 0, blocks: 2..<3)])
}

@Test("a shared file ends the phase and stands alone")
func aSharedFileEndsThePhaseAndStandsAlone() {
    let t = one([shell(), call(.shared(path: "chart.png", caption: ""), .ok), shell()])
    #expect(Array(rows(t, pending: 0, sending: false)[2..<5]) == [.phase(turn: 0, blocks: 0..<1), .shared(turn: 0, block: 1), .phase(turn: 0, blocks: 2..<3)])
}

@Test("a turn the agent begins itself heads its own reply")
func aTurnTheAgentBeginsItselfHeadsItsOwnReply() {
    let t = Transcript(turns: [
        turn("u1", [agent()], done), own("n1", [shell(), text("reported")], done), own("c", [.compacted(id: "c", auto: true)], done),
        own("n2", [text("resumed")], nil),
    ])
    let folds = rows(t, pending: 0, sending: false).compactMap { if case .fold(let turn) = $0 { turn } else { nil } }
    #expect(folds == [0, 1, 3], "a compaction gets none")
    #expect(foldOf(t, 1) == 1 && folded(t, 0) == 0..<1)
    #expect(foldOf(t, 3) == 3 && folded(t, 3) == 3..<4)
}

@Test("a reply is at work while its turn is open or a sub-agent it started runs")
func aReplyIsAtWorkWhileItsTurnIsOpenOrASubAgentItStartedRuns() {
    let t = Transcript(turns: [turn("u1", [agent(), agent()], done), turn("u2", [shell()], nil)])
    #expect(atWorkOn(t, 0..<1) && atWorkOn(t, 1..<2))
    #expect(inBackground(t.turns[0..<1]).count == 2)
    #expect(inBackground(t.turns[1..<2]).isEmpty, "an open turn's agents aren't in the background yet")
    #expect(!atWorkOn(one([shell()]), 0..<1))
}

@Test("a phase is titled by its work")
func aPhaseIsTitledByItsWork() {
    let edit = { (p: String) in call(.edit(path: p, diff: nil), .ok) }
    let read = call(.read(path: "a"), .ok)
    #expect(phaseTitle([edit("src/a.rs"), edit("src/a.rs"), shell()], live: false) == "Edited a.rs · Ran a command")
    #expect(phaseTitle([read, edit("a"), edit("b"), shell(), shell()], live: true) == "Editing 2 files · Exploring the project · Executing 2 commands")
    #expect(phaseTitle([think()], live: false) == "Thought")
    #expect(phaseTitle([call(.other(input: "", description: ""), .ok)], live: false) == "Working")
    let sub = { (s: CallStatus) in call(.agent(SubAgent(description: "")), s) }
    #expect(phaseTitle([sub(.running)], live: false) == "Using a subagent", "it runs on in the background")
    #expect(phaseTitle([sub(.ok), shell()], live: true) == "Used a subagent · Executing a command")
    #expect(phaseTitle([sub(.ok), sub(.failed)], live: false) == "Used 2 subagents")
}

private func parts(_ md: String) -> [String] {
    let u = Array(md.utf8)
    return markdownBlocks(md).map { String(decoding: u[$0], as: UTF8.self) }
}

@Test("a long message becomes one row per markdown block")
func aLongMessageBecomesOneRowPerMarkdownBlock() {
    let md = "## Step 1\nIntro line\nmore intro\n\n- one\n- two\n\n- three\n  continued\n\nLast."
    #expect(parts(md) == ["## Step 1", "Intro line\nmore intro", "- one\n- two\n\n- three\n  continued", "Last."])
    #expect(rows(one([text(md)]), pending: 0, sending: false).filter { if case .text = $0 { true } else { false } }.count == 4)
}

@Test("a fenced code block is never split")
func aFencedCodeBlockIsNeverSplit() {
    let md = "Run:\n```sh\nmake\n\n## not a heading\n```\nAfter\n\n~~~~\n```\n\n~~~~\n\n```unclosed\n\nrest"
    #expect(parts(md) == ["Run:", "```sh\nmake\n\n## not a heading\n```", "After", "~~~~\n```\n\n~~~~", "```unclosed\n\nrest"])
}

@Test("inline backticks open no fence")
func inlineBackticksOpenNoFence() {
    #expect(markdownBlocks("```inline``` code\n\nnext").count == 2)
}

@Test("row ids stay the same when a tool finishes")
func rowIdsStayTheSameWhenAToolFinishes() {
    let running = Transcript(turns: [turn("u1", [call(.shell(command: "ls"), .running)], nil)])
    let finished = Transcript(turns: [turn("u1", [shell(), text("done")], nil)])
    #expect(ids(running) == ["u1", "u1#fold", "u1#p0", "u1#live"])
    #expect(ids(finished) == ["u1", "u1#fold", "u1#p0", "u1#1.0", "u1#live"])
}

@Test("exchanges get fourteen and their parts six")
func exchangesGetFourteenAndTheirPartsSix() {
    let t = Transcript(turns: [turn("u1", [shell(), text("a\n\nb"), .compacted(id: "c", auto: true)], done), turn("u2", [], nil)])
    let r = rows(t, pending: 1, sending: true)
    let gaps = r.indices.map { topGap($0 > 0 ? r[$0 - 1] : nil, r[$0]) }
    // prompt, fold, phase, a, b, compacted, end, prompt, fold, pending, sending, live
    #expect(gaps == [16, 6, 0, 6, 10, 8, 6, 14, 6, 14, 0, 6])
}

@Test("splice replaces only rows after the shared prefix")
func spliceReplacesOnlyRowsAfterTheSharedPrefix() {
    #expect(splice(["a", "b", "working"], ["a", "b", "c", "working"]) == (2..<3, 2))
    #expect(splice(["a", "b"], ["a", "b"]) == (2..<2, 0))
    #expect(splice(["a", "b"], ["a"]) == (1..<2, 0))
}

@Test("sending shows only while a send is on its way")
func sendingShowsOnlyWhileASendIsOnItsWay() {
    let open = Transcript(turns: [turn("u1", [], done), turn("u2", [shell()], nil)])
    #expect(rows(open, pending: 0, sending: false).last == .live(turn: 1), "the live line says the agent works")
    let ended = one([shell()])
    #expect(rows(ended, pending: 0, sending: false).last == .end(turn: 0))
    #expect(Array(rows(ended, pending: 1, sending: true)[4...]) == [.pending(0), .working])
    #expect(Array(rows(ended, pending: 1, sending: false)[4...]) == [.pending(0)], "a send not delivered waits on no one")
    let compacted = Transcript(turns: [own("c", [.compacted(id: "c", auto: true)], nil)])
    #expect(rows(compacted, pending: 0, sending: false) == [.compacted(turn: 0, block: 0)])
}

@Test("the live line trails only the last reply while the agent works on it")
func theLiveLineTrailsOnlyTheLastReplyWhileTheAgentWorksOnIt() {
    let t = Transcript(turns: [turn("u1", [agent()], done), turn("u2", [shell()], done), own("n1", [shell()], nil)])
    #expect(liveTail(t) == 2, "the turn the agent began itself heads its own reply")
    #expect(rows(t, pending: 1, sending: true).suffix(4) == [.phase(turn: 2, blocks: 0..<1), .pending(0), .working, .live(turn: 2)], "a send waits above the live line")
    #expect(liveTail(one([shell()])) == nil)
    #expect(liveTail(Transcript()) == nil)
}

@Test("a live tail settles once its fold stops carrying the live line")
func aLiveTailSettlesOnceItsFoldStopsCarryingTheLiveLine() {
    let working = Transcript(turns: [turn("u1", [agent()], done), turn("u2", [shell()], done), own("n1", [shell()], nil)])
    var ended = Transcript(turns: [turn("u1", [agent()], done), turn("u2", [shell()], done), own("n1", [shell()], done)])
    #expect(settledTail("n1", working) == nil)
    #expect(settledTail("n1", ended) == nil, "u1's sub-agent still works in the background")
    ended.turns[0].blocks = [call(.agent(SubAgent(description: "")), .ok)]
    #expect(settledTail("n1", ended) == "n1")
    #expect(settledTail(nil, ended) == nil)
}

@Test("a command left in the background counts but leaves the agent idle")
func aCommandLeftInTheBackgroundCountsButLeavesTheAgentIdle() {
    let t = Transcript(turns: [turn("u1", [call(.shell(command: "bun run dev"), .running), shell()], done)])
    #expect(inBackground(t.turns[...]).count == 1)
    #expect(!atWorkOn(t, 0..<1))
}

@Test("a background task is found at its turn and block and shown by its row")
func aBackgroundTaskIsFoundAtItsTurnAndBlockAndShownByItsRow() {
    let t = Transcript(turns: [turn("u1", [shell()], done), turn("u2", [shell(), call(.shell(command: "bun run dev"), .running), agent()], done)])
    #expect(inBackground(t.turns[...]).map { [$0.turn, $0.block] } == [[1, 1], [1, 2]])
    let r = rows(t, pending: 0, sending: false)
    let shown = { (turn: Int, block: Int) in rowOf(r, turn: turn, block: block).map { r[$0] } }
    #expect(shown(1, 1) == .phase(turn: 1, blocks: 0..<3), "a command shows in the phase that holds it")
    #expect(shown(1, 2) == .phase(turn: 1, blocks: 0..<3), "and so does a sub-agent")
    #expect(shown(1, 5) == nil)
}

@Test("a background sub-agent keeps the agent at work after its turn ends")
func aBackgroundSubAgentKeepsTheAgentAtWorkAfterItsTurnEnds() {
    let sub = { (s: CallStatus) in call(.agent(SubAgent(description: "")), s) }
    #expect(atWorkOn(Transcript(turns: [turn("u1", [sub(.running)], done), turn("u2", [shell()], done)]), 0..<1), "its turn ended, but its sub-agent runs on")
    #expect(!atWorkOn(Transcript(turns: [turn("u1", [sub(.ok)], done)]), 0..<1))
}

@Test("the live line follows the newest exchange while a sub-agent runs in the background")
func theLiveLineFollowsTheNewestExchangeWhileASubAgentRunsInTheBackground() {
    let t = Transcript(turns: [turn("u1", [agent()], done), turn("u2", [shell()], done)])
    #expect(liveTail(t) == 1)
    #expect(rows(t, pending: 0, sending: false).last == .live(turn: 1))
}

@Test("a turn the agent begins after a background sub-agent finished reports it")
func aTurnTheAgentBeginsAfterABackgroundSubAgentFinishedReportsIt() {
    let sub = { (at: Int, ms: Int) in Block.tool(Call(id: "a", name: "", kind: .agent(SubAgent(description: "")), status: .ok, durationMs: ms, at: at)) }
    let reporting = own("n1", [.text(Message(id: "r", at: 10_500, text: "It found two."))], done)
    let ended = { (blocks: [Block]) in turn("u1", blocks, .done(durationMs: 1_000, usage: nil)) }
    let t = Transcript(turns: [ended([sub(100, 10_000)]), reporting])
    #expect(reported(t, 1).count == 1)
    #expect(Array(ids(t)[..<2]) == ["u1", "u1#fold"])
    #expect(ids(t).contains("n1#notice"))
    #expect(reported(Transcript(turns: [ended([sub(100, 500)]), reporting]), 1).isEmpty, "the turn waited for it, so it reported in that turn")
}

@Test("each turn the agent begins itself is a reply timed on its own")
func eachTurnTheAgentBeginsItselfIsAReplyTimedOnItsOwn() {
    let reporting = { (id: String, ms: Int, input: Int) in
        own(id, [text("a sub-agent reported")], .done(durationMs: ms, usage: Usage(input: input, output: 1, cacheRead: 0)))
    }
    let t = Transcript(turns: [
        turn("u1", [agent()], done), reporting("n1", 5_000, 10), reporting("n2", 7_000, 20), own("c", [.compacted(id: "c", auto: true)], done),
        turn("u2", [shell()], nil),
    ])
    #expect(reply(t, 0) == 0..<1 && reply(t, 1) == 1..<2 && reply(t, 2) == 2..<3, "a footer under each")
    #expect(reply(t, 3) == 3..<4)
    #expect(worked(t, 2..<3)?.ms == 7_000 && worked(t, 2..<3)?.usage == Usage(input: 20, output: 1, cacheRead: 0))
    #expect(worked(t, 4..<5) == nil, "an open turn hasn't worked yet")
}

@Test("what set the agent going heads its turn, and an error it went on after stands in place")
func whatSetTheAgentGoingHeadsItsTurnAndAnErrorItWentOnAfterStandsInPlace() {
    let notice = { (text: String, error: Bool) in Block.notice(msg(text), error: error) }
    let t = Transcript(turns: [
        turn("u1", [text("Hi.")], done), own("n1", [notice("Stop hook: run the tests", false), shell(), notice("API Error", true), shell(), text("Done.")], done),
    ])
    #expect(Array(ids(t)[4...]) == ["n1#0", "n1#fold", "n1#p1", "n1#2", "n1#p3", "n1#4.0", "n1#end"])
    #expect(rows(t, pending: 0, sending: false)[4] == .note(turn: 1, block: 0))
}

@Test("a command that finished in the background is reported like a sub-agent")
func aCommandThatFinishedInTheBackgroundIsReportedLikeASubAgent() {
    let cmd = Block.tool(Call(id: "b", name: "", kind: .shell(command: "cargo test"), status: .failed, durationMs: 10_000, at: 100))
    let t = Transcript(turns: [turn("u1", [cmd], .done(durationMs: 1_000, usage: nil)), own("n1", [.text(Message(id: "r", at: 10_500, text: "It failed."))], done)])
    #expect(reported(t, 1).count == 1)
}

@Test("a call that finished after its turn failed is reported by the turn it set going")
func aCallThatFinishedAfterItsTurnFailedIsReportedByTheTurnItSetGoing() {
    let cmd = { (ms: Int) in Block.tool(Call(id: "b", name: "", kind: .shell(command: "cargo test"), status: .ok, durationMs: ms, at: 100)) }
    let failed = { (blocks: [Block]) in turn("u1", blocks, .failed(error: "API Error: Connection lost", durationMs: 1_000)) }
    let reporting = own("n1", [.text(Message(id: "r", at: 10_500, text: "Tests pass."))], done)
    #expect(reported(Transcript(turns: [failed([cmd(10_000)]), reporting]), 1).count == 1)
    #expect(reported(Transcript(turns: [failed([cmd(500)]), reporting]), 1).isEmpty, "it ended before the error did")
}

@Test("a prompt sent mid-turn carries on the reply it came into")
func aPromptSentMidTurnCarriesOnTheReplyItCameInto() {
    let t = Transcript(turns: [turn("u1", [think(), shell()], nil), turn("u2", [shell(), text("Done.")], done)])
    #expect(ids(t) == ["u1", "u1#fold", "u1#p0", "u2", "u2#p0", "u2#1.0", "u2#end"], "the steer stands between the work it divides")
    #expect(foldOf(t, 1) == 0 && folded(t, 0) == 0..<2)
    #expect(reply(t, 0) == nil && reply(t, 1) == 0..<2, "one footer for the whole reply")
    #expect(worked(t, 0..<2)?.ms == 1 && worked(t, 0..<2)?.usage == nil, "the agent times the turn once")
}

@Test("a reply stays at work while the agent works past a steer")
func aReplyStaysAtWorkWhileTheAgentWorksPastASteer() {
    let t = Transcript(turns: [turn("u1", [think(), shell()], nil), turn("u2", [shell()], nil)])
    #expect(atWorkOn(t, folded(t, 0)), "the work before the steer stays open")
    #expect(liveTail(t) == 0)
}

@Test("a step up lands on the showing prompt before the one above it")
func aStepUpLandsOnTheShowingPromptBeforeTheOneAboveIt() {
    var r = [Row](repeating: .pending(0), count: 21)
    for ix in [2, 10, 20] { r[ix] = .prompt(turn: ix) }
    #expect(stepPrompt(r, 12, down: false) == 10, "back to the start of the exchange showing")
    #expect(stepPrompt(r, 10, down: false) == 2)
    #expect(stepPrompt(r, 2, down: false) == nil)
    #expect(stepPrompt(r, 12, down: true) == 20)
    #expect(stepPrompt(r, 0, down: true) == 2, "above every prompt, down goes to the first")
    #expect(stepPrompt(r, 20, down: true) == nil)
}
