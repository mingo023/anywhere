import Foundation
import Testing
import Wire

@testable import ChatModel

private func onlyTool(_ t: Transcript) -> Call {
    #expect(t.turns[0].blocks.count == 1)
    return t.turns[0].blocks[0].call!
}

private func subAgent(_ c: Call) -> Transcript { c.agent!.transcript }

@Test("a user item starts a turn and a result ends it")
func aUserItemStartsATurnAndAResultEndsIt() {
    let t = fold(items(#"""
        [{"kind": "user", "text": "hi"}, {"kind": "assistant", "text": "hello"},
         {"kind": "result", "ok": true, "durationMs": 5, "usage": {"inputTokens": 1, "outputTokens": 2, "cacheReadTokens": 3}},
         {"kind": "user", "text": "again"}]
        """#))
    #expect(t.turns.count == 2)
    let first = t.turns[0]
    #expect(first.id == "i1")
    #expect(first.prompt == Message(id: "i1", at: 1000, text: "hi"))
    #expect(first.blocks == [.text(Message(id: "i2", at: 2000, text: "hello"))])
    #expect(first.end == .done(durationMs: 5, usage: Usage(input: 1, output: 2, cacheRead: 3)))
    #expect(t.turns[1].prompt?.text == "again")
    #expect(t.turns[1].end == nil)
}

@Test("items before the first prompt form a turn without one")
func itemsBeforeTheFirstPromptFormATurnWithoutOne() {
    let t = fold(items(#"""
        [{"kind": "assistant", "text": "picking up"}, {"kind": "result", "ok": true},
         {"kind": "compact", "trigger": "auto"}, {"kind": "user", "text": "next"}]
        """#))
    #expect(t.turns.count == 3)
    #expect(t.turns[0].id == "i1" && t.turns[0].prompt == nil)
    #expect(t.turns[0].end != nil)
    #expect(t.turns[1].id == "i3" && t.turns[1].prompt == nil)
    #expect(t.turns[1].blocks == [.compacted(id: "i3", auto: true)])
    #expect(t.turns[2].prompt != nil)
}

@Test("a tool update replaces the running call with the same use id")
func aToolUpdateReplacesTheRunningCallWithTheSameUseId() {
    let t = fold(items(#"""
        [{"kind": "user", "text": "go"},
         \#(toolJSON("tu1", "Bash", #"{"kind": "shell", "command": "ls"}"#, "running")),
         {"kind": "assistant", "text": "meanwhile"},
         {"kind": "tool", "call": {"toolUseId": "tu1", "name": "Bash", "detail": {"kind": "shell", "command": "ls"}, "status": "ok", "output": "a\nb", "durationMs": 7}}]
        """#))
    let blocks = t.turns[0].blocks
    #expect(blocks.count == 2)
    let call = blocks[0].call!
    #expect(call.id == "tu1" && call.status == .ok && call.output == "a\nb" && call.durationMs == 7)
    #expect(call.kind == .shell(command: "ls"))
}

@Test("a tool end after a new prompt updates the call in its own turn")
func aToolEndAfterANewPromptUpdatesTheCallInItsOwnTurn() {
    let shell = #"{"kind": "shell", "command": "make"}"#
    let t = fold(items("""
        [{"kind": "user", "text": "one"}, \(toolJSON("tu1", "Bash", shell, "running")),
         {"kind": "user", "text": "two"}, \(toolJSON("tu1", "Bash", shell, "error"))]
        """))
    #expect(t.turns[0].blocks[0].call?.status == .failed)
    #expect(t.turns[1].blocks.isEmpty)
}

@Test("tools without a use id never replace each other")
func toolsWithoutAUseIdNeverReplaceEachOther() {
    let t = fold(items("""
        [\(toolJSON("", "Bash", #"{"kind": "shell", "command": "a"}"#, "ok")), \(toolJSON("", "Bash", #"{"kind": "shell", "command": "b"}"#, "ok"))]
        """))
    #expect(t.turns[0].blocks.count == 2)
}

@Test("a failed result ends the turn with its error")
func aFailedResultEndsTheTurnWithItsError() {
    let t = fold(items(#"[{"kind": "user", "text": "deploy"}, {"kind": "result", "ok": false, "durationMs": 9000, "error": "API Error: 529 Overloaded"}]"#))
    #expect(t.turns[0].end == .failed(error: "API Error: 529 Overloaded", durationMs: 9000))
}

@Test("a result with no prompt ends a turn of its own")
func aResultWithNoPromptEndsATurnOfItsOwn() {
    let t = fold(items(#"[{"kind": "user", "text": "stop"}, {"kind": "result", "ok": true}, {"kind": "result", "ok": false, "error": "interrupted"}]"#))
    #expect(t.turns.count == 2)
    #expect(t.turns[1].prompt == nil)
    #expect(t.turns[1].end == .failed(error: "interrupted", durationMs: 0))
}

@Test("items out of seq order fold by seq")
func itemsOutOfSeqOrderFoldBySeq() {
    let t = fold(items(#"[{"kind": "assistant", "text": "reply", "seq": 2}, {"kind": "user", "text": "ask", "seq": 1}]"#))
    #expect(t.turns.count == 1)
    #expect(t.turns[0].prompt?.text == "ask")
    #expect(t.turns[0].blocks.count == 1)
}

@Test("blank text and thinking are left out")
func blankTextAndThinkingAreLeftOut() {
    let t = fold(items(#"[{"kind": "user", "text": "hi"}, {"kind": "thinking", "text": "  "}, {"kind": "assistant", "text": "\n"}]"#))
    #expect(t.turns[0].blocks.isEmpty)
}

@Test("webfetch and websearch become fetch and search")
func webfetchAndWebsearchBecomeFetchAndSearch() {
    let t = fold(items("""
        [\(toolJSON("f", "WebFetch", #"{"kind": "other", "name": "WebFetch", "input": {"url": "https://x.dev", "prompt": "why"}}"#, "ok")),
         \(toolJSON("s", "WebSearch", #"{"kind": "other", "name": "WebSearch", "input": {"query": "gpui list"}}"#, "ok")),
         \(toolJSON("g", "Grep", #"{"kind": "search", "query": "fn group", "path": "src"}"#, "ok")),
         \(toolJSON("m", "mcp__linear__create", #"{"kind": "other", "name": "mcp__linear__create", "input": {"team": "Desktop"}}"#, "ok"))]
        """))
    let kinds = t.turns[0].blocks.map { $0.call!.kind }
    #expect(kinds[0] == .fetch(url: "https://x.dev"))
    #expect(kinds[1] == .search(query: "gpui list"))
    #expect(kinds[2] == .search(query: "fn group in src"))
    #expect(kinds[3] == .other(input: "{\n  \"team\": \"Desktop\"\n}", description: ""))
}

@Test("another tool keeps what pocketd says it is about")
func anotherToolKeepsWhatPocketdSaysItIsAbout() {
    let t = fold(items("[\(toolJSON("s", "TaskStop", #"{"kind": "other", "name": "TaskStop", "input": {"task_id": "a1"}, "description": "Audit"}"#, "ok"))]"))
    #expect(t.turns[0].blocks[0].call?.kind == .other(input: "{\n  \"task_id\": \"a1\"\n}", description: "Audit"))
}

@Test("ask user question becomes a question tool")
func askUserQuestionBecomesAQuestionTool() {
    let input = #"{"questions": [{"question": "Which parser?", "header": "Parser", "multiSelect": true, "options": [{"label": "comrak", "description": "GFM"}]}]}"#
    let t = fold(items("[\(toolJSON("q", "AskUserQuestion", #"{"kind": "other", "name": "AskUserQuestion", "input": \#(input)}"#, "running"))]"))
    let question = Question(header: "Parser", question: "Which parser?", options: [Choice(label: "comrak", description: "GFM")], multi: true)
    #expect(onlyTool(t).kind == .question([question]))
}

@Test("a task call becomes a sub-agent")
func aTaskCallBecomesASubAgent() {
    let t = fold(items("[\(toolJSON("tu-t", "Task", #"{"kind": "task", "description": "Audit packages/app"}"#, "running"))]"))
    let call = onlyTool(t)
    #expect(call.kind == .agent(SubAgent(description: "Audit packages/app")))
    #expect(call.status == .running)
}

@Test("a task call carries the context its sub-agent answered from")
func aTaskCallCarriesTheContextItsSubAgentAnsweredFrom() {
    let t = fold(items("[\(toolJSON("tu-t", "Task", #"{"kind": "task", "description": "Look", "model": "claude-haiku-5-5", "tokens": 32900}"#, "running"))]"))
    let agent = onlyTool(t).agent!
    #expect(agent.model == "claude-haiku-5-5" && agent.tokens == 32900)
}

@Test("items with a parent fold into their sub-agent's transcript")
func itemsWithAParentFoldIntoTheirSubAgentsTranscript() {
    let t = fold(items("""
        [{"kind": "user", "text": "audit"}, \(toolJSON("tu-t", "Task", #"{"kind": "task", "description": "Audit"}"#, "running")),
         {"kind": "tool", "parentToolUseId": "tu-t", "call": {"toolUseId": "tu-c", "name": "Read", "detail": {"kind": "read", "path": "Cargo.toml"}, "status": "ok"}},
         {"kind": "assistant", "text": "back in the parent"},
         {"kind": "assistant", "text": "unused crates", "parentToolUseId": "tu-t"}]
        """))
    let blocks = t.turns[0].blocks
    #expect(blocks.count == 2)
    let sub = subAgent(blocks[0].call!)
    #expect(sub.turns.count == 1)
    #expect(sub.turns[0].prompt == nil)
    #expect(sub.turns[0].blocks.count == 2)
    #expect(sub.turns[0].blocks[0].call?.id == "tu-c")
    #expect(sub.turns[0].blocks[1] == .text(Message(id: "i5", at: 5000, text: "unused crates")))
}

@Test("a sub-agent's own sub-agent nests two deep")
func aSubAgentsOwnSubAgentNestsTwoDeep() {
    let t = fold(items("""
        [\(toolJSON("a", "Task", #"{"kind": "task", "description": "outer"}"#, "running")),
         {"kind": "tool", "parentToolUseId": "a", "call": {"toolUseId": "b", "name": "Task", "detail": {"kind": "task", "description": "inner"}, "status": "running"}},
         {"kind": "assistant", "text": "deep", "parentToolUseId": "b"}]
        """))
    let inner = subAgent(onlyTool(t)).turns[0].blocks[0].call!
    #expect(subAgent(inner).turns[0].blocks == [.text(Message(id: "i3", at: 3000, text: "deep"))])
}

@Test("a claude session pocketd paged nests each sub-agent under its own agent call")
func aClaudeSessionPocketdPagedNestsEachSubAgentUnderItsOwnAgentCall() throws {
    let data = try JSONSerialization.data(withJSONObject: golden("timeline_subagents")["items"]!)
    let t = fold(try JSONDecoder().decode([TimelineItem].self, from: data))
    #expect(t.turns.count == 1)
    let blocks = t.turns[0].blocks
    try #require(blocks.count == 3)
    guard case .text(let answer) = blocks[2] else { Issue.record("an answer"); return }
    #expect(answer.text.hasPrefix("The repo holds"))
    #expect(blocks[0].call?.agent?.model == "claude-haiku-4-5-20251001", "the model it replied as")
    let list = subAgent(blocks[0].call!)
    #expect(list.turns[0].prompt?.text == "Run ls in /w and report what is there")
    #expect(list.turns[0].blocks.first?.call?.kind == .shell(command: "ls /w"))
    #expect(list.turns[0].blocks.first?.call?.status == .ok)
    #expect(list.turns[0].blocks.last == .text(Message(id: "i6", at: 6, text: "README.md and src/")))
    #expect(subAgent(blocks[1].call!).turns[0].blocks == [.text(Message(id: "i8", at: 8, text: "1, 2, 3"))])
}

@Test("items whose parent is no sub-agent are left out")
func itemsWhoseParentIsNoSubAgentAreLeftOut() {
    let t = fold(items("""
        [\(toolJSON("r", "Read", #"{"kind": "read", "path": "a"}"#, "ok")),
         {"kind": "assistant", "text": "orphan", "parentToolUseId": "r"}, {"kind": "assistant", "text": "lost", "parentToolUseId": "gone"}]
        """))
    #expect(t.turns.count == 1)
    #expect(t.turns[0].blocks.count == 1)
}

@Test("a tasks item becomes todos with mapped statuses")
func aTasksItemBecomesTodosWithMappedStatuses() {
    let t = fold(items(#"""
        [{"kind": "tasks", "items": [{"text": "read", "status": "completed"}, {"text": "group", "status": "in_progress"}, {"text": "test", "status": "pending"}]}]
        """#))
    #expect(t.turns[0].blocks == [.todos(id: "i1", todos: [Todo(text: "read", status: .done), Todo(text: "group", status: .active), Todo(text: "test", status: .pending)])])
}

@Test("a compact item becomes a compacted marker")
func aCompactItemBecomesACompactedMarker() {
    let t = fold(items(#"[{"kind": "user", "text": "go"}, {"kind": "compact", "trigger": "manual"}]"#))
    #expect(t.turns[0].blocks == [.compacted(id: "i2", auto: false)])
}

@Test("a notice after an ended turn heads a turn the agent began itself")
func aNoticeAfterAnEndedTurnHeadsATurnTheAgentBeganItself() {
    let t = fold(items(#"""
        [{"kind": "user", "text": "go"}, {"kind": "result", "ok": true, "durationMs": 5},
         {"kind": "notice", "text": "Stop hook: tests fail"}, {"kind": "assistant", "text": "Fixing them."}]
        """#))
    #expect(t.turns.count == 2)
    #expect(t.turns[1].prompt == nil)
    #expect(t.turns[1].blocks[0] == .notice(Message(id: "i3", at: 3000, text: "Stop hook: tests fail"), error: false))
}

@Test("an error the agent went on after is a notice in its turn")
func anErrorTheAgentWentOnAfterIsANoticeInItsTurn() {
    var notice = golden("stream_notice_error")["item"] as! [String: Any]
    notice["seq"] = 2
    let t = fold([items(#"[{"kind": "user", "text": "go"}]"#)[0], decode(notice)])
    #expect(t.turns.count == 1)
    #expect(t.turns[0].end == nil)
    #expect(t.turns[0].blocks == [.notice(Message(id: "i8", at: 17, text: "API Error: 500"), error: true)])
}

@Test("an edit keeps its diff")
func anEditKeepsItsDiff() {
    let item = decode(golden("stream_tool_done")["item"] as! [String: Any])
    let diff = FileDiff(lines: [DiffLine(number: 1, kind: .del, text: "a"), DiffLine(number: 1, kind: .add, text: "b")], additions: 1, deletions: 1)
    #expect(onlyTool(fold([item])).kind == .edit(path: "a.go", diff: diff))
}

@Test("a permission maps to an ask with its tool")
func aPermissionMapsToAnAskWithItsTool() throws {
    let data = try JSONSerialization.data(withJSONObject: golden("permission_request")["request"]!)
    let p = try JSONDecoder().decode(PermissionRequest.self, from: data)
    #expect(ask(p) == Ask(requestId: "r1", tool: .shell(command: "rm x"), toolName: "Bash", questions: []))
}

@Test("a share_file call becomes a shared file")
func aShareFileCallBecomesASharedFile() {
    let detail = #"{"kind": "other", "name": "\#(SharedFile.tool)", "input": {"path": "out/chart.png", "caption": "Q3 revenue"}}"#
    let t = fold(items("[\(toolJSON("sh", SharedFile.tool, detail, "ok"))]"))
    #expect(onlyTool(t).kind == .shared(path: "out/chart.png", caption: "Q3 revenue"))
}
