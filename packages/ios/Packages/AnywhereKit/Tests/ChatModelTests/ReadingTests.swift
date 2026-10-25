import Testing
import Wire
@testable import ChatModel

private func msg(_ text: String, _ at: Int) -> Message { Message(id: text, at: at, text: text) }

private func tool(_ kind: ToolKind, _ status: CallStatus, _ at: Int) -> Block {
    .tool(Call(id: "t\(at)", name: "x", kind: kind, status: status, at: at))
}

private func shell(_ at: Int) -> Block { tool(.shell(command: "ls"), .ok, at) }

private func turn(_ blocks: [Block], _ end: End?) -> Transcript {
    Transcript(turns: [Turn(id: "a", prompt: msg("go", 1000), blocks: blocks, end: end)])
}

private let done = End.done(durationMs: 1, usage: nil)

@Test("a live fold says who works and for how long")
func aLiveFoldSaysWhoWorks() {
    #expect(liveLabel(who("claude"), "Opus 5.5", .working(secs: 7, background: 0)) == "Opus 5.5 working for 7s")
    #expect(liveLabel(who("codex"), "", .working(secs: 64, background: 0)) == "Codex working for 1m 4s")
    #expect(liveLabel("Claude", "Opus 5.5", .working(secs: 3, background: 1)) == "Opus 5.5 working for 3s · in the background")
    #expect(liveLabel("Claude", "Opus 5.5", .working(secs: 3, background: 2)) == "Opus 5.5 working for 3s · 2 in the background")
    #expect(liveLabel("Claude", "Opus 5.5", .thinking) == "Thinking…")
    #expect(liveLabel("Claude", "Opus 5.5", .stopping) == "Stopping…")
    #expect(liveLabel("Claude", "Opus 5.5", .approval) == "Needs your approval")
    #expect(liveLabel("Claude", "Opus 5.5", .answer) == "Needs your answer")
}

@Test("a settled fold says how long the agent worked")
func aSettledFoldSaysHowLong() {
    #expect(settledLabel("Claude", 64_900) == "Claude worked for 1m 4s")
}

@Test("live thinking keeps its tail and settled thinking its head")
func liveThinkingKeepsItsTail() {
    let text = (1...30).map { "step \($0)\n" }.joined()
    let live = thinkingLines(text, live: true)
    #expect(live.lines.first == "step 7" && live.lines.last == "step 30" && live.more == 6)
    let settled = thinkingLines(text, live: false)
    #expect(settled.lines.first == "step 1" && settled.lines.last == "step 24" && settled.more == 6)
}

@Test("a long thought wraps between words at 96 columns")
func aLongThoughtWraps() {
    let l = thinkingLines(String(repeating: "word ", count: 40), live: false).lines
    #expect(l.count == 3 && l.allSatisfy { $0.count <= 96 && !$0.hasPrefix(" ") })
}

@Test("output over 24 lines keeps the head and counts the rest")
func outputOver24Lines() {
    let text = (1...30).map { "line \($0)\n" }.joined() + "\n\n"
    let out = DetailLines.output(text)
    #expect(out.lines.count == DetailLines.max)
    #expect(out.lines[0] == "line 1" && out.lines[23] == "line 24")
    #expect(out.more == 6 && out.moreLabel == "… 6 more lines")
    #expect(DetailLines.output("ok\n").more == 0)
}

@Test("a users choice overrides a chips default")
func aUsersChoiceOverrides() {
    var d = Details()
    #expect(d.open("thought", true) && !d.open("read", false))
    let open = d.toggle("thought", true)
    #expect(!open)
    #expect(!d.open("thought", true))
}

@Test("todos count done of total")
func todosCountDoneOfTotal() {
    let t = { Todo(text: "x", status: $0) }
    #expect(todoCount([t(.done), t(.active), t(.pending), t(.done)]) == (2, 4))
    #expect(todoCount([]) == (0, 0))
}

@Test("the tasks card reads done of total until complete")
func theTasksCardReads() {
    let t = { Todo(text: "x", status: $0) }
    #expect(todoProgress([t(.done), t(.active), t(.pending)]) == "1 of 3")
    #expect(todoProgress([t(.done), t(.done)]) == "Complete")
}

@Test("a turn the user stopped reads as interrupted not failed")
func aTurnTheUserStopped() {
    #expect(interrupted("Interrupted by user"))
    #expect(interrupted("[Request interrupted by user]"))
    #expect(!interrupted("API Error: 529 overloaded"))
}

@Test("a question waits until answered then shows its header")
func aQuestionWaits() {
    let ask = { (status: CallStatus, header: String) in
        Call(id: "t", name: "AskUserQuestion", kind: .question([Question(header: header, question: "Which  parser?")]), status: status)
    }
    #expect(questionValue(ask(.running, "Parser")) == "Awaiting your answer…")
    #expect(questionValue(ask(.ok, "Parser")) == "Parser")
    #expect(questionValue(ask(.ok, " ")) == "Which parser?")
}

@Test("a permission card names its tool and path")
func aPermissionCardNames() {
    #expect(permissionSubject(.edit(path: "src/lib.rs", diff: nil), "Edit") == (.pencil, "Edit", "src/lib.rs"))
    #expect(permissionSubject(.shell(command: "rm -rf build"), "Bash") == (.terminal, "Run", "rm -rf build"))
    #expect(permissionSubject(.other(input: "{}", description: ""), "mcp__db__query") == (.blocks, "Use", "mcp__db__query"))
}

@Test("each tool reads as a verb and what it acted on")
func eachToolReadsAsAVerb() {
    let call = { (name: String, kind: ToolKind) in Call(id: "t", name: name, kind: kind, status: .ok) }
    let other = { (d: String) in ToolKind.other(input: "{}", description: d) }
    let cases: [(Call, String, Arg)] = [
        (call("Read", .read(path: "src/a.rs")), "Read", .file("src/a.rs")),
        (call("Edit", .edit(path: "b.ts", diff: nil)), "Edit", .file("b.ts")),
        (call("Write", .write(path: "c.md", diff: nil)), "Create", .file("c.md")),
        (call("Grep", .search(query: "fn  group\n")), "Find", .text("fn group")),
        (call("Bash", .shell(command: "\ncargo test\nmore")), "", .raw("cargo test")),
        (call("Bash", .shell(command: "")), "", .raw("Shell")),
        (call("mcp__linear__list", other("")), "", .raw("linear · list")),
        (call("Skill", .other(input: #"{"skill": "apple-design"}"#, description: "")), "Skill", .text("/apple-design")),
        (call("NotebookEdit", other("")), "", .raw("NotebookEdit")),
        (call("Monitor", .shell(command: "tail -f build.log")), "Watch", .raw("tail -f build.log")),
        (call("TaskStop", other("Standards review\nof quit diff")), "Stop", .text("Standards review of quit diff")),
        (call("TaskStop", .shell(command: "bun run dev")), "Stop", .raw("bun run dev")),
        (call("TaskOutput", .shell(command: "bun run dev")), "Read output of", .raw("bun run dev")),
    ]
    for (c, verb, arg) in cases {
        #expect(toolLine(c) == (verb, arg), "\(c.name)")
    }
}

private func subTool(_ id: String, _ status: CallStatus) -> Block {
    .tool(Call(id: id, name: "Bash", kind: .shell(command: "ls"), status: status))
}

private func agent(_ blocks: [Block]) -> SubAgent {
    SubAgent(description: "Audit deps", transcript: Transcript(turns: [Turn(id: "t", blocks: blocks)]))
}

private func call(_ a: SubAgent, _ status: CallStatus, _ output: String?) -> Call {
    Call(id: "c", name: "Task", kind: .agent(a), status: status, output: output)
}

@Test("a sub agents name is its tasks first line cut past 56 chars")
func aSubAgentsName() {
    let a = { SubAgent(description: $0) }
    #expect(Agent.name(a("\n  Check the fold\nthen more")) == "Check the fold")
    #expect(Agent.name(a(String(repeating: "x", count: 56))) == String(repeating: "x", count: 56))
    #expect(Agent.name(a(String(repeating: "x", count: 57))) == String(repeating: "x", count: 55) + "…")
    #expect(Agent.name(a(" ")) == "Subagent")
}

@Test("a sub agent starts until it does something then counts its steps if it shows any")
func aSubAgentStarts() {
    let quiet = agent([])
    #expect(Agent.mark(call(quiet, .running, nil), quiet) == .starting)
    #expect(Agent.meta(call(quiet, .running, nil), quiet) == "starting")
    let busy = agent([subTool("a", .ok), subTool("b", .failed), subTool("c", .running)])
    #expect(Agent.mark(call(busy, .running, nil), busy) == .running)
    #expect(Agent.meta(call(busy, .running, nil), busy) == "3 steps, 1 failed")
    let one = agent([subTool("a", .ok)])
    #expect(Agent.meta(call(one, .ok, nil), one) == "1 step")
    #expect(Agent.meta(call(one, .failed, nil), one) == "failed")
    #expect(Agent.meta(call(quiet, .ok, "Done."), quiet) == "done")
}

@Test("a sub agent reports its output else its last words once done")
func aSubAgentReports() {
    let text = { (s: String) in Block.text(Message(id: s, at: 0, text: s)) }
    let a = agent([text("first"), subTool("a", .ok), text("left-pad is unused")])
    #expect(Agent.report(call(a, .running, nil), a) == nil)
    #expect(Agent.report(call(a, .ok, nil), a) == "left-pad is unused")
    #expect(Agent.report(call(a, .ok, " Found 2 \n"), a) == "Found 2")
}

@Test("a sub agent waiting on its own work shows what it said so far")
func aSubAgentWaiting() {
    let a = agent([subTool("a", .running)])
    #expect(Agent.report(call(a, .running, " Started the build; waiting on it.\n"), a) == "Started the build; waiting on it.")
    #expect(Agent.report(call(a, .running, "  "), a) == nil)
}

@Test("a sub agents brief is the prompt it started on")
func aSubAgentsBrief() {
    var a = agent([])
    #expect(Agent.brief(a) == nil)
    a.transcript.turns[0].prompt = Message(id: "p", at: 0, text: "\n Review the diff.\n")
    #expect(Agent.brief(a) == "Review the diff.")
    a.transcript.turns[0].prompt = Message(id: "p", at: 0, text: " ")
    #expect(Agent.brief(a) == nil)
}

@Test("a running sub agent shows its newest call as a step")
func aRunningSubAgentShowsItsNewestCall() {
    var read = Call(id: "r", name: "Read", kind: .read(path: "docs/spec.md"), status: .ok)
    let a = agent([subTool("a", .ok), .tool(read), .text(Message(id: "", at: 0, text: ""))])
    #expect(Agent.latest(a).map(Agent.step) == "Read docs/spec.md")
    read.kind = .shell(command: "\ncargo test\n")
    #expect(Agent.step(read) == "cargo test")
    #expect(Agent.latest(agent([])) == nil)
}

@Test("a text counts the lines it wraps to")
func aTextCountsTheLines() {
    #expect(Agent.lineCount("one") == 1)
    #expect(Agent.lineCount("one\n\nthree") == 3)
    #expect(Agent.lineCount(String(repeating: "x", count: 91)) == 2)
}

@Test("a phase is headed by the kind of its first step")
func aPhaseIsHeaded() {
    let t = { (kind: ToolKind) in Block.tool(Call(id: "t", name: "x", kind: kind, status: .ok)) }
    #expect(Phase.glyph([t(.read(path: "a")), t(.shell(command: "ls"))]) == .search)
    #expect(Phase.glyph([t(.edit(path: "a", diff: nil))]) == .pencil)
    #expect(Phase.glyph([t(.shell(command: "ls"))]) == .terminal)
    #expect(Phase.glyph([.thinking(Message(id: "", at: 0, text: ""))]) == .bulb)
    #expect(Phase.glyph([t(.other(input: "{}", description: ""))]) == .tool)
}

@Test("a phases steps leave out its todos")
func aPhasesStepsLeaveOutTodos() {
    let blocks: [Block] = [
        .tool(Call(id: "t", name: "x", kind: .read(path: "a"), status: .ok)), .todos(id: "t", todos: []),
        .thinking(Message(id: "", at: 0, text: "")),
    ]
    #expect(Phase.steps(blocks[...]).map(\.index) == [0, 2])
}

@Test("a thought lasts until the agents next move")
func aThoughtLasts() {
    #expect(Phase.thoughtFor(1000, 4500) == "Thought for 3s")
    #expect(Phase.thoughtFor(1000, nil) == nil)
    #expect(Phase.thoughtFor(1000, 1999) == nil)
}

@Test("a notice names the command that finished in the background and how")
func aNoticeNamesTheCommand() {
    let c = { (name: String, kind: ToolKind, status: CallStatus) in Call(id: "t", name: name, kind: kind, status: status) }
    let cmd = ToolKind.shell(command: "cargo test\n--workspace")
    #expect(noticeLabel([c("Bash", cmd, .failed)]) == "Background command `cargo test` failed")
    #expect(noticeLabel([c("Bash", cmd, .ok)]) == "Background command `cargo test` is done")
    #expect(noticeLabel([c("Monitor", cmd, .ok)]) == "Monitor `cargo test` ended")
    let agent = c("Agent", .agent(SubAgent(description: "")), .ok)
    #expect(noticeLabel([agent, agent]) == "2 subagents are done")
    #expect(noticeLabel([agent, c("Bash", cmd, .ok)]) == "2 background tasks are done")
}

@Test("a notice keeps a command with backticks in one code span")
func aNoticeKeepsBackticks() {
    #expect(codeSpan("Background command `echo `date`` failed") == ("Background command ", "echo `date`", " failed"))
    #expect(codeSpan("2 subagents are done") == ("2 subagents are done", nil, ""))
}

@Test("only text that grew since the last update streams in")
func onlyTextThatGrewStreamsIn() {
    let text = { (id: String, s: String) in Block.text(Message(id: id, at: 0, text: s)) }
    let t = turn([text("same", "word word"), text("grew", "word word word"), text("new", "word word")], nil)
    var reveals: [String: Reveal] = [:]
    streamIn(&reveals, before: ["same": 9, "grew": 4], t, now: 0)
    #expect(reveals["same"] == nil)
    #expect(reveals["grew"]?.cut("word word word", 0) == 4)
    #expect(reveals["new"]?.cut("word word", 0) == 0)
    streamIn(&reveals, before: textLengths(t), t, now: 2)
    #expect(reveals.isEmpty)
}

@Test("content width clamps between 560 and 1200 in steps of 16")
func contentWidthClamps() {
    #expect(contentWidth(300) == 560)
    #expect(contentWidth(5000) == 1200)
    #expect(contentWidth(745) == 752)
    #expect(contentWidth(720) == 720)
    #expect(contentWidth(.nan) == 720)
}

@Test("a live fold thinks until the agent acts and waits on the user first")
func aLiveFoldThinks() {
    let thinking = turn([.thinking(msg("hm", 2000))], nil)
    #expect(liveState(thinking, 0..<1, ask: nil, stopping: false, now: 9000) == .thinking)
    let working = turn([.thinking(msg("hm", 2000)), shell(3000)], nil)
    #expect(liveState(working, 0..<1, ask: nil, stopping: false, now: 8500) == .working(secs: 7, background: 0))
    #expect(liveState(working, 0..<1, ask: false, stopping: false, now: 8500) == .approval)
    #expect(liveState(working, 0..<1, ask: true, stopping: false, now: 8500) == .answer)
}

@Test("a live fold reads stopping from the stop on")
func aLiveFoldReadsStopping() {
    let thinking = turn([.thinking(msg("hm", 2000))], nil)
    #expect(liveState(thinking, 0..<1, ask: nil, stopping: true, now: 9000) == .stopping)
    let working = turn([shell(3000)], nil)
    #expect(liveState(working, 0..<1, ask: false, stopping: true, now: 8500) == .stopping)
}

@Test("a settled fold counts the turns time else its first to last move")
func aSettledFoldCounts() {
    #expect(took(turn([shell(3000)], .done(durationMs: 64_000, usage: nil)), 0..<1) == 64_000)
    #expect(took(turn([shell(3000), shell(9000)], .failed(error: "interrupted", durationMs: 0)), 0..<1) == 8000)
}

@Test("a phase is live until its turn writes or works on past it")
func aPhaseIsLive() {
    #expect(phaseLive(turn([shell(1), .todos(id: "d", todos: [])], nil), 0, 0..<1))
    #expect(!phaseLive(turn([shell(1), .text(msg("Done.", 2))], nil), 0, 0..<1))
    #expect(!phaseLive(turn([shell(1)], done), 0, 0..<1))
}

@Test("a live fold counts the sub agents working on in the background")
func aLiveFoldCountsSubAgents() {
    let t = turn([tool(.agent(SubAgent(description: "")), .running, 2000)], done)
    #expect(liveState(t, 0..<1, ask: nil, stopping: false, now: 5000) == .working(secs: 4, background: 1))
}

@Test("a phase hangs off its fold only once the live line leaves it")
func aPhaseHangsOffItsFold() {
    let a = { (status: CallStatus) in tool(.agent(SubAgent(description: "")), status, 1) }
    let phase = Row.phase(turn: 0, blocks: 0..<1)
    let live = turn([a(.running)], done)
    #expect(!inFold(live, liveTail: liveTail(live), phase, foldOpen: true))
    let settled = turn([a(.ok)], done)
    #expect(inFold(settled, liveTail: liveTail(settled), phase, foldOpen: true))
}

@Test("a closed fold hides its work but not a failed sub agent")
func aClosedFoldHidesItsWork() {
    let a = { (status: CallStatus) in tool(.agent(SubAgent(description: "")), status, 1) }
    let t = turn([shell(1), a(.ok), a(.failed)], nil)
    #expect(!shown(t, .phase(turn: 0, blocks: 0..<2), foldOpen: false))
    #expect(shown(t, .phase(turn: 0, blocks: 2..<3), foldOpen: false))
    #expect(shown(t, .phase(turn: 0, blocks: 0..<2), foldOpen: true))
}

@Test("a closed fold still shows the text between its work")
func aClosedFoldStillShowsText() {
    let t = turn([.text(msg("Let me look.", 1)), shell(2), .text(msg("All good.", 3))], done)
    let text = { Row.text(turn: 0, block: $0, range: 0..<1) }
    #expect(shown(t, text(0), foldOpen: false) && shown(t, text(2), foldOpen: false))
    #expect(inFold(t, liveTail: nil, text(0), foldOpen: true) && !inFold(t, liveTail: nil, text(0), foldOpen: false))
    #expect(!inFold(t, liveTail: nil, text(2), foldOpen: true))
}

@Test("a replys answer leaves out the text between its work")
func aReplysAnswer() {
    let t = turn([.text(msg("Let me look.", 1)), shell(2), .text(msg("All good.", 3))], done)
    #expect(answer(t, 0..<1) == "All good.")
}

@Test("labels models like the picker")
func labelsModelsLikeThePicker() {
    #expect(modelLabel(model: "claude-opus-5", provider: "claude") == "Opus 5")
    #expect(modelLabel(model: "claude-opus-5-5", provider: "claude") == "Opus 5.5")
    #expect(modelLabel(model: "claude-haiku-4-5-20251001", provider: "claude") == "Haiku 4.5")
    #expect(modelLabel(model: nil, provider: "codex") == "Codex")
}
