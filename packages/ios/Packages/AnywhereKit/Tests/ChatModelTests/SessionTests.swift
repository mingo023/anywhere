import Fixtures
import Foundation
import Testing
import Wire

@testable import ChatModel

private func bash(_ seq: Int, _ id: String, _ status: String) -> TimelineItem {
    let output = String(repeating: "x\\n", count: 40)
    return item(seq, #"{"kind": "tool", "call": {"toolUseId": "\#(id)", "name": "Bash", "status": "\#(status)", "output": "\#(output)", "detail": {"kind": "shell", "command": "make"}}}"#)
}

private let perf = ProcessInfo.processInfo.environment["CHAT_PERF"] != nil
private let go = #"{"kind": "user", "text": "go"}"#
private let ok = #"{"kind": "result", "ok": true}"#

@Test("showing a row opens the shut fold over it and its shut phase, but no fold work holds open")
func showingARowOpensTheShutFoldOverItAndItsShutPhaseButNoFoldWorkHoldsOpen() {
    var s = Session()
    s.update(1, [item(1, go), bash(2, "t1", "ok"), item(3, ok)], working: false, now: 0)
    let phase = s.rows.firstIndex { if case .phase = $0 { true } else { false } }!
    #expect(s.shutFoldOver(phase) == "i1#fold" && s.shutPhase(phase))
    s.toggle("i1#fold", false)
    s.toggle(s.ids[phase], false)
    #expect(s.shutFoldOver(phase) == nil && !s.shutPhase(phase))

    let task = #"{"kind": "tool", "call": {"toolUseId": "tu", "name": "Task", "status": "running", "detail": {"kind": "task", "description": "audit"}}}"#
    var t = Session()
    t.update(1, [item(1, go), item(2, task), item(3, ok)], working: false, now: 0)
    let agent = t.rows.firstIndex { if case .phase = $0 { true } else { false } }!
    #expect(t.shutFoldOver(agent) == nil, "its sub-agent still at work holds the fold open")
}

@Test("a session refolds only when the revision moves")
func aSessionRefoldsOnlyWhenTheRevisionMoves() {
    var s = Session()
    var items = [item(1, go)]
    #expect(s.update(1, items, working: false, now: 0) == Change(0..<0, 0..<0, 3))
    #expect(s.ids == ["i1", "i1#fold", "i1#live"])
    items.append(item(2, #"{"kind": "assistant", "text": "on it"}"#))
    #expect(s.update(1, items, working: false, now: 0) == nil, "same revision, nothing refolded")
    #expect(s.transcript.turns[0].blocks.isEmpty)
    #expect(s.update(2, items, working: false, now: 0) == Change(2..<2, 2..<3, 2), "the prompt and fold rows stay")
    #expect(s.ids == ["i1", "i1#fold", "i1#0.0", "i1#live"])
}

@Test("a tool finishing replaces its turn's rows though their ids hold")
func aToolFinishingReplacesItsTurnsRowsThoughTheirIdsHold() {
    var s = Session()
    var items = [item(1, go), bash(2, "t1", "running")]
    s.update(1, items, working: false, now: 0)
    items[1] = bash(2, "t1", "ok")
    #expect(s.update(2, items, working: false, now: 0) == Change(2..<4, 4..<4, 0), "the prompt and fold stay, the phase and the live line under it are measured afresh")
    #expect(s.update(3, items, working: false, now: 0) == nil, "a refold that changes nothing splices nothing")
}

@Test("streaming text replaces only the markdown block that grew")
func streamingTextReplacesOnlyTheMarkdownBlockThatGrew() {
    var s = Session()
    var items = [item(1, go), item(2, #"{"kind": "assistant", "text": "a\n\nb"}"#)]
    s.update(1, items, working: false, now: 0)
    items[1] = item(2, #"{"kind": "assistant", "text": "a\n\nbc"}"#)
    #expect(s.update(2, items, working: false, now: 0) == Change(3..<5, 5..<5, 0))
}

@Test("an agent with no items shows no rows and no working row")
func anAgentWithNoItemsShowsNoRowsAndNoWorkingRow() {
    var s = Session()
    #expect(s.update(1, [], working: false, now: 0) == nil)
    #expect(s.rows.isEmpty && s.ids.isEmpty)
}

@Test("a streamed reply grows in one row until its end follows it")
func aStreamedReplyGrowsInOneRowUntilItsEndFollowsIt() {
    var s = Session()
    var items = [item(1, go)]
    s.update(1, items, working: false, now: 0)
    #expect(s.ids == ["i1", "i1#fold", "i1#live"])
    for (n, text) in ["The", "The transcript", "The transcript jumped."].enumerated() {
        items = [items[0], item(2, #"{"kind": "assistant", "text": "\#(text)"}"#)]
        let change = s.update(2 + n, items, working: false, now: 0)
        #expect(s.ids == ["i1", "i1#fold", "i1#0.0", "i1#live"], "chunk \(n)")
        #expect(ChatModel.text(s.transcript.turns[0], 0) == text)
        if n > 0 { #expect(change == Change(2..<4, 4..<4, 0), "only the growing reply and the live line under it are re-measured") }
    }
    items.append(item(3, #"{"kind": "result", "ok": true, "durationMs": 1200}"#))
    #expect(s.update(9, items, working: false, now: 0) == Change(3..<3, 3..<4, 1), "the end takes the live line's place")
    #expect(s.ids == ["i1", "i1#fold", "i1#0.0", "i1#end"])
}

@Test("a pending send adds its row without a refold")
func aPendingSendAddsItsRowWithoutARefold() {
    var s = Session()
    let items = [item(1, go), item(2, ok)]
    s.update(1, items, working: false, now: 0)
    s.outgoing.push("next", working: false, now: 10)
    #expect(s.update(1, items, working: false, now: 10) == Change(3..<3, 3..<3, 2))
    #expect(Array(s.ids[3...]) == ["pending-0", "working"])
}

@Test("a send not delivered drops the sending trailer")
func aSendNotDeliveredDropsTheSendingTrailer() {
    var s = Session()
    let items = [item(1, go), item(2, ok)]
    s.update(1, items, working: false, now: 0)
    s.outgoing.push("next", working: false, now: 10)
    s.update(1, items, working: false, now: 10)
    #expect(s.update(1, items, working: false, now: 200_000) == Change(4..<4, 4..<5, 0))
    #expect(Array(s.ids[3...]) == ["pending-0"])
}

@Test("a queued send waits off the transcript until its turn ends")
func aQueuedSendWaitsOffTheTranscriptUntilItsTurnEnds() {
    var s = Session()
    var items = [item(1, go)]
    s.update(1, items, working: false, now: 0)
    s.outgoing.push("next", working: true, now: 10)
    #expect(s.update(1, items, working: false, now: 10) == nil, "the composer's tray shows it")
    items.append(item(2, ok))
    s.update(2, items, working: false, now: 20)
    #expect(Array(s.ids[3...]) == ["pending-0", "working"])
}

@Test("a new conversation keeps queued sends and fails none")
func aNewConversationKeepsQueuedSendsAndFailsNone() {
    var s = Session()
    let old = [item(1, go)]
    s.update(1, old, working: false, now: 0)
    s.outgoing.push("next", working: true, now: 10)
    s.update(1, old, working: false, now: 10)
    s.update(2, [], working: false, now: 20)
    #expect(s.transcript.turns.isEmpty && !s.ids.contains("i1"), "the old conversation's rows are gone")
    #expect(s.outgoing.released().map(\.text) == ["next"], "the new conversation is idle, so the queue goes on")
    let new = [item(5, #"{"kind": "user", "text": "next"}"#)]
    s.update(3, new, working: false, now: 30)
    s.update(3, new, working: false, now: 200_000)
    #expect(s.outgoing.list.isEmpty)
}

@Test("a shown transcript splices only when it changes")
func aShownTranscriptSplicesOnlyWhenItChanges() {
    let t = fold([item(1, go), item(2, #"{"kind": "assistant", "text": "on it"}"#)])
    var s = Session()
    #expect(s.show(t) == Change(0..<0, 0..<0, 4))
    #expect(s.ids == ["i1", "i1#fold", "i1#0.0", "i1#live"])
    #expect(s.show(t) == nil)
}

@Test("toggling a row flips what it shows from its default")
func togglingARowFlipsWhatItShowsFromItsDefault() {
    var s = Session()
    #expect(s.open("u1#p0", true))
    s.toggle("u1#p0", true)
    #expect(!s.open("u1#p0", true))
    s.toggle("u1#p0", true)
    #expect(s.open("u1#p0", false), "the user's choice outlasts the default")
    s.toggle("u1#fold", false)
    #expect(s.open("u1#fold", false))
}

@Test("a pocketd restart mid-turn holds a queued send")
func aPocketdRestartMidTurnHoldsAQueuedSend() {
    var s = Session()
    let working = [item(1, #"{"kind": "user", "text": "long job"}"#), item(2, #"{"kind": "assistant", "text": "on it"}"#)]
    s.update(1, working, working: true, now: 0)
    s.outgoing.push("next", working: true, now: 10)
    s.update(1, working, working: true, now: 10)
    // pocketd restarted: its resets moved, so the held items were dropped until it sends them again.
    s.update(2, [], working: true, now: 20)
    #expect(s.outgoing.released().isEmpty, "the agent still works")
    s.update(3, working, working: true, now: 30)
    #expect(s.outgoing.released().isEmpty)
}

@Test("a command reads as work until the timeline moves")
func aCommandReadsAsWorkUntilTheTimelineMoves() {
    var s = Session()
    var items = [item(1, go), item(2, ok)]
    s.update(1, items, working: false, now: 0)
    s.outgoing.push("/compact", working: false, now: 10)
    s.update(1, items, working: true, now: 20)
    #expect(busy(s.transcript, s.outgoing, working: true))
    items.append(item(3, #"{"kind": "compact", "trigger": "manual"}"#))
    s.update(2, items, working: true, now: 4_000)
    #expect(!busy(s.transcript, s.outgoing, working: true), "the compaction shows; status trails it")
}

/// Takes `item` in as the chat store does: replacing the one with its id, then in seq order. Returns how many items
/// before it it left alone.
@discardableResult
func upsert(_ items: inout [TimelineItem], _ item: TimelineItem) -> Int {
    let old = items.firstIndex { $0.id == item.id }
    if let old { items[old] = item } else { items.append(item) }
    items.sort { $0.seq < $1.seq }
    let new = items.firstIndex { $0.id == item.id }!
    return min(old ?? new, new)
}

/// The change from rebuilding and comparing every row, as if the session refolded everything.
private func fullSplice(_ old: Transcript, _ new: Transcript) -> Change? {
    let (oldRows, newRows) = (rows(old, pending: 0, sending: false), rows(new, pending: 0, sending: false))
    let (range, count) = splice(oldRows.map { $0.id(old) }, newRows.map { $0.id(new) })
    let first = (0..<range.lowerBound).first { oldRows[$0] != newRows[$0] || !sameContent(old.turns, 0, new, newRows[$0]) } ?? range.lowerBound
    return first < range.upperBound || count > 0 ? Change(first..<range.lowerBound, range, count) : nil
}

/// Feeds the session one item at a time, checking each refold against folding everything afresh; `hinted` says
/// which items each update left alone, as the chat store does.
private func replay(_ updates: [[String: Any]], hinted: Bool = false) {
    var s = Session()
    var items: [TimelineItem] = []
    var shown = Transcript()
    for (n, v) in updates.enumerated() {
        let unchanged = upsert(&items, decode(v))
        let change = s.update(n + 1, items, working: false, now: 0, unchanged: hinted ? unchanged : 0)
        let full = fold(items)
        let r = rows(full, pending: 0, sending: false)
        #expect(s.transcript == full, "update \(n)")
        #expect(s.rows == r, "update \(n)")
        #expect(s.ids == r.map { $0.id(full) }, "update \(n)")
        #expect(change == fullSplice(shown, full), "update \(n)")
        if s.transcript != full { return }
        shown = full
    }
}

/// A scenario's items as pocketd streams them, ids prefixed: text grows in chunks, and a finished call
/// first runs until the next item lands.
private func streamed(_ items: [[String: Any]], _ prefix: String, _ seq: inout Int) -> [[String: Any]] {
    var out: [[String: Any]] = []
    var finishing: [String: Any]?
    for var v in items {
        seq += 1
        v["seq"] = seq
        v["ts"] = seq
        v["id"] = prefix + (v["id"] as! String)
        if let p = v["parentToolUseId"] as? String { v["parentToolUseId"] = prefix + p }
        if var call = v["call"] as? [String: Any], let id = call["toolUseId"] as? String {
            call["toolUseId"] = prefix + id
            v["call"] = call
        }
        if let text = v["text"] as? String {
            let u = Array(text.utf8)
            let spaces = u.indices.filter { u[$0] == 0x20 }
            for cut in stride(from: 0, to: spaces.count, by: 7).map({ spaces[$0] }) {
                var partial = v
                partial["text"] = String(decoding: u[..<cut], as: UTF8.self)
                out.append(partial)
            }
        }
        if var call = v["call"] as? [String: Any], let status = call["status"] as? String, status != "running" {
            var running = v
            call["status"] = "running"
            call["output"] = nil
            running["call"] = call
            out.append(running)
            if let f = finishing { out.append(f) }
            finishing = v
        } else {
            out.append(v)
            if let f = finishing { out.append(f) }
            finishing = nil
        }
    }
    if let f = finishing { out.append(f) }
    return out
}

private func scenarios() -> [(String, [[String: Any]])] {
    let all = Sample.chatScenarios.map { ($0.name, try! JSONSerialization.jsonObject(with: Data($0.json.utf8)) as! [[String: Any]]) }
    #expect(all.count >= 10)
    return all.sorted { $0.0 < $1.0 }
}

@Test("refolding each fixture scenario as it streams matches folding it whole")
func refoldingEachFixtureScenarioAsItStreamsMatchesFoldingItWhole() {
    var chained: [[String: Any]] = []
    var chainedSeq = 0
    for (name, list) in scenarios() {
        var seq = 0
        replay(streamed(list, "", &seq))
        chained += streamed(list, "\(name)-", &chainedSeq)
    }
    replay(chained)
}

/// A long session as pocketd streams it: replies grow in place, calls finish later (some after the next prompt,
/// some as a new item), sub-agents two deep report into running calls, and now and then an item lands between older ones.
func generated(_ count: Int) -> [[String: Any]] {
    var rng: UInt64 = 0x2545_f491_4f6c_dd1d
    func roll(_ n: Int) -> Int {
        rng ^= rng << 13
        rng ^= rng >> 7
        rng ^= rng << 17
        return Int(rng % UInt64(n))
    }
    var out: [[String: Any]] = []
    var running: [[String: Any]] = []
    var agents: [String] = []
    for n in 0..<count {
        let id = "g\(n)"
        let seq = 10 * n + 10
        var v: [String: Any]
        let r = roll(100)
        switch r {
        case 0..<8: v = ["kind": "user", "text": "prompt \(n)"]
        case 8..<10: v = ["kind": "result", "ok": roll(4) > 0, "error": "interrupted"]
        case 10..<30: v = ["kind": "assistant", "text": "reply \(n)\n\n- one\n- two\n\nthe end of \(n)"]
        case 30..<35: v = ["kind": "thinking", "text": "thought \(n)"]
        case 35..<55:
            let tools: [(String, [String: Any])] = [
                ("Bash", ["kind": "shell", "command": "make"]), ("Read", ["kind": "read", "path": "a.rs"]), ("Task", ["kind": "task", "description": "audit"]),
            ]
            let (name, detail) = tools[roll(3)]
            if name == "Task" { agents.append("tu\(n)") }
            v = ["kind": "tool", "call": ["toolUseId": "tu\(n)", "name": name, "status": "running", "detail": detail]]
        case 55..<70 where !running.isEmpty:
            let ix = roll(5) == 0 ? roll(running.count) : running.count - 1
            var c = running.remove(at: ix)
            var call = c["call"] as! [String: Any]
            agents.removeAll { $0 == call["toolUseId"] as! String }
            call["status"] = ["ok", "error"][roll(2)]
            call["output"] = "out \(n)"
            c["call"] = call
            if roll(2) == 0 {
                out.append(c)
                continue
            }
            v = ["kind": "tool", "call": call]
        case 70..<82 where !agents.isEmpty:
            let parent = agents[roll(5) == 0 ? roll(agents.count) : agents.count - 1]
            if roll(5) == 0 {
                agents.append("tu\(n)")
                v = ["kind": "tool", "call": ["toolUseId": "tu\(n)", "name": "Task", "status": "running", "detail": ["kind": "task", "description": "deeper"]]]
            } else {
                v = ["kind": "assistant", "text": "sub \(n)"]
            }
            v["parentToolUseId"] = parent
        case 82..<84: v = ["kind": "tasks", "items": [["text": "todo \(n)", "status": "in_progress"]]]
        case 84..<85: v = ["kind": "compact", "trigger": "auto"]
        case 85..<87: v = ["kind": "assistant", "text": "  "]
        case 87..<89: v = ["kind": "assistant", "text": "orphan \(n)", "parentToolUseId": "gone"]
        case 89..<91: v = ["kind": "assistant", "text": "late \(n)", "seq": 10 * roll(n + 1) + 5]
        default: v = ["kind": "assistant", "text": "short \(n)"]
        }
        v["id"] = id
        v["ts"] = seq
        if v["seq"] == nil { v["seq"] = seq }
        if v["kind"] as? String == "tool", (v["call"] as? [String: Any])?["status"] as? String == "running" { running.append(v) }
        if v["kind"] as? String == "assistant", v["parentToolUseId"] == nil {
            let u = Array((v["text"] as! String).utf8)
            for cut in u.indices where u[cut] == 0x0A {
                var partial = v
                partial["text"] = String(decoding: u[..<cut], as: UTF8.self)
                out.append(partial)
            }
        }
        out.append(v)
    }
    return out
}

@Test("refolding a thousand item stream item by item matches folding it whole")
func refoldingAThousandItemStreamItemByItemMatchesFoldingItWhole() {
    let updates = generated(1000)
    #expect(updates.count > 1000)
    replay(updates)
    replay(updates, hinted: true)
}

/// Minutes in a debug build, where each of its updates folds everything afresh to compare: `CHAT_PERF=1 swift test -c release`.
@Test("refolding a five thousand item stream item by item matches folding it whole", .enabled(if: perf))
func refoldingAFiveThousandItemStreamItemByItemMatchesFoldingItWhole() {
    let updates = generated(5000)
    #expect(updates.count > 5000)
    replay(updates)
}

/// `turns` finished turns of a prompt, thinking, a reply, three tools, a closing reply and a result: 8 items each.
func history(_ turns: Int) -> [TimelineItem] {
    var out: [TimelineItem] = []
    var seq = 0
    func add(_ v: [String: Any]) {
        seq += 1
        var v = v
        v["seq"] = seq
        v["id"] = "i\(seq)"
        v["ts"] = seq
        out.append(decode(v))
    }
    let long = (0..<12).map { "## Step \($0)\n\nA paragraph about it.\n\n- one\n- two\n\n" }.joined()
    let lines: [[String: Any]] = (0..<20).map { ["number": $0, "kind": $0 % 2 == 0 ? "add" : "del", "text": "let x = 1;"] }
    for turn in 0..<turns {
        add(["kind": "user", "text": "turn \(turn)"])
        add(["kind": "thinking", "text": String(repeating: "considering ", count: 40)])
        add(["kind": "assistant", "text": turn % 5 == 0 ? long : "On it."])
        add(["kind": "tool", "call": ["toolUseId": "b\(turn)", "name": "Bash", "status": "ok", "output": String(repeating: "x\n", count: 40), "detail": ["kind": "shell", "command": "make"]]])
        add(["kind": "tool", "call": ["toolUseId": "r\(turn)", "name": "Read", "status": "ok", "output": String(repeating: "line\n", count: 200), "detail": ["kind": "read", "path": "src/lib.rs"]]])
        add([
            "kind": "tool",
            "call": ["toolUseId": "e\(turn)", "name": "Edit", "status": "ok", "detail": ["kind": "edit", "path": "src/lib.rs", "diff": ["lines": lines, "additions": 10, "deletions": 10]]],
        ])
        add(["kind": "assistant", "text": "Done."])
        add(["kind": "result", "ok": true, "durationMs": 1200])
    }
    return out
}

/// p50, p95 and max of `update` while a reply streams into a new turn after `n` items of history, in µs.
func streaming(_ n: Int, hinted: Bool = false) -> (p50: Int, p95: Int, max: Int) {
    var items = history(n / 8)
    var s = Session()
    s.update(1, items, working: false, now: 0)
    let seq = items.count
    items.append(item(seq + 1, #"{"kind": "user", "text": "stream"}"#))
    items.append(item(seq + 2, #"{"kind": "assistant", "text": ""}"#))
    var took: [Int] = []
    var text = ""
    for chunk in 0..<200 {
        text += chunk % 20 == 19 ? "end.\n\n" : "word "
        items[items.count - 1].body = .assistant(text: text)
        let start = DispatchTime.now().uptimeNanoseconds
        _ = s.update(2 + chunk, items, working: false, now: 0, unchanged: hinted ? items.count - 1 : 0)
        took.append(Int(DispatchTime.now().uptimeNanoseconds - start) / 1000)
    }
    took.sort()
    return (took[took.count / 2], took[took.count * 95 / 100], took[took.count - 1])
}

/// `CHAT_PERF=1 swift test -c release --filter ChatModelTests`
@Test("a reply streaming after twelve hundred or five thousand items refolds well under two ms", .enabled(if: perf))
func aReplyStreamingAfterTwelveHundredOrFiveThousandItemsRefoldsWellUnderTwoMs() {
    for n in [1200, 5000] {
        let (p50, p95, max) = streaming(n)
        print("\(n) items: update p50 \(p50)µs p95 \(p95)µs max \(max)µs")
        #expect(p50 < 2000 && p95 < 2000, "\(n): \(p50)µs/\(p95)µs")
        let hinted = streaming(n, hinted: true)
        print("\(n) items, told what changed: update p50 \(hinted.p50)µs p95 \(hinted.p95)µs max \(hinted.max)µs")
    }
}

/// `CHAT_PERF=1 swift test -c release --filter ChatModelTests`
@Test("folding twelve hundred items takes well under two ms", .enabled(if: perf))
func foldingTwelveHundredItemsTakesWellUnderTwoMs() {
    let items = history(150)
    #expect(items.count == 1200)
    var s = Session()
    s.update(1, items, working: false, now: 0)
    let runs = 50
    var start = DispatchTime.now().uptimeNanoseconds
    for revision in 2..<2 + runs { _ = s.update(revision, items, working: false, now: 0) }
    let each = Int(DispatchTime.now().uptimeNanoseconds - start) / runs / 1000
    start = DispatchTime.now().uptimeNanoseconds
    s.update(1 + runs, items, working: false, now: 0)
    let idle = Int(DispatchTime.now().uptimeNanoseconds - start) / 1000
    print("update: \(each)µs per refold, \(idle)µs unchanged")
    #expect(each < 2000, "\(each)µs")
}
