import Testing
@testable import ChatModel

private func agent(_ id: String, _ status: CallStatus) -> Block {
    .tool(Call(id: id, name: "Agent", kind: .agent(SubAgent(description: "")), status: status, durationMs: 27_000, at: 1000))
}

private func shell(_ id: String) -> Block {
    .tool(Call(id: id, name: "Bash", kind: .shell(command: "cargo test"), status: .ok))
}

private func turn(_ blocks: [Block], _ ended: Bool) -> Turn {
    Turn(id: "t", blocks: blocks, end: ended ? .done(durationMs: 1, usage: nil) : nil)
}

@Test("escape leaves the details then closes the map which stays drawn while it fades")
func escapeLeavesTheDetails() {
    var nav = AgentMap.Nav()
    nav.toggle("a", 0)
    nav.show(2, "c2")
    nav.escape(0)
    #expect(nav.isOpen("a") && nav.detail == nil)
    #expect(nav.shown == "c2")
    nav.escape(0)
    #expect(!nav.isOpen("a") && nav.drawn("a", 0))
    #expect(!nav.drawn("a", AgentMap.shutting))
}

@Test("reopening the map starts on the graph with the main agent picked")
func reopeningTheMap() {
    var nav = AgentMap.Nav()
    nav.toggle("a", 0)
    nav.show(3, "c3")
    nav.toggle("a", 0)
    nav.toggle("a", 0)
    #expect(nav.selected == 0 && nav.detail == nil && nav.shown == nil)
}

@Test("the arrow keys stop at the first and last node")
func theArrowKeysStop() {
    var nav = AgentMap.Nav()
    nav.step(down: false, 3)
    #expect(nav.selected == 0)
    for _ in 0..<5 { nav.step(down: true, 3) }
    #expect(nav.selected == 2)
}

@Test("enter opens the picked agent and left goes back to the graph")
func enterOpensThePickedAgent() {
    var nav = AgentMap.Nav()
    nav.toggle("a", 0)
    let calls = ["c1", "c2"]
    var took = nav.key(.enter, calls, 0)
    #expect(!took)
    took = nav.key(.down, calls, 0) && nav.key(.down, calls, 0) && nav.key(.enter, calls, 0)
    #expect(took)
    #expect(nav.detail == "c2")
    took = nav.key(.down, calls, 0)
    #expect(!took)
    took = nav.key(.left, calls, 0)
    #expect(took && nav.detail == nil)
    took = nav.key(.enter, ["c1"], 0)
    #expect(!took)
}

@Test("a node says how long its agent ran and the context it used")
func aNodeSaysHowLongItsAgentRan() {
    var c = agent("a", .ok).call!
    #expect(AgentMap.meta(c, tokens: 32_900, now: 0) == "27s · 32.9k tokens")
    #expect(AgentMap.meta(c, tokens: 0, now: 0) == "27s")
    c.status = .running
    #expect(AgentMap.meta(c, tokens: 640, now: 16_000) == "15s · 640 tokens")
    c.status = .failed
    #expect(AgentMap.meta(c, tokens: 9, now: 0) == "Failed after 27s")
    #expect(AgentMap.tokens(1000) == "1.0k tokens")
}

@Test("the chip counts the agents still working out of all")
func theChipCountsTheAgents() {
    let t = Transcript(turns: [turn([agent("a", .running), agent("b", .ok), agent("c", .running)], true)])
    #expect(AgentMap.chipLabel(spawned(t)) == (true, "2 of 3 agents"))
    let done = Transcript(turns: [turn([agent("a", .ok)], true)])
    #expect(AgentMap.chipLabel(spawned(done)) == (false, "1 agent"))
    let all = Transcript(turns: [turn([agent("a", .running), agent("b", .running)], false)])
    #expect(AgentMap.chipLabel(spawned(all)) == (true, "2 agents"))
    #expect(AgentMap.summary(spawned(t)) == "3 agents · 2 working")
}

@Test("the main agent hands off to the agents its turn ends with")
func theMainAgentHandsOff() {
    let waiting = Transcript(turns: [turn([shell("s"), agent("a", .running), agent("b", .running)], false)])
    #expect(AgentMap.mainLine(waiting, spawned(waiting)) == "Handed off to 2 agents")
    let on = Transcript(turns: [turn([agent("a", .ok), shell("s")], false)])
    #expect(AgentMap.mainLine(on, spawned(on)) == "cargo test")
    let left = Transcript(turns: [turn([agent("a", .running)], true)])
    #expect(AgentMap.mainLine(left, spawned(left)) == "Turn ended · agents still working")
    let done = Transcript(turns: [turn([agent("a", .ok)], true)])
    #expect(AgentMap.mainLine(done, spawned(done)) == "Done")
}

@Test("a nested agent shows in the chat at the call its ancestor was spawned under")
func aNestedAgentShowsAtItsAncestorsCall() {
    let inner = Transcript(turns: [turn([agent("inner", .ok)], true)])
    var outer = agent("outer", .ok).call!
    outer.kind = .agent(SubAgent(description: "", transcript: inner))
    let t = Transcript(turns: [turn([shell("s")], true), turn([shell("x"), .tool(outer)], true)])
    let place = AgentMap.place(t, spawned(t), 1)
    #expect(place?.turn == 1 && place?.block == 1)
}

@Test("a step names its tool beside what it acted on")
func aStepNamesItsTool() {
    #expect(AgentMap.step(Call(id: "c", name: "Read", kind: .read(path: "src/a.rs"), status: .ok)) == ("Read", "src/a.rs"))
    #expect(AgentMap.step(Call(id: "c", name: "Bash", kind: .shell(command: "cargo test\n"), status: .ok)) == ("Bash", "cargo test"))
}

@Test("the details say where the agent stands and its model")
func theDetailsSayWhereTheAgentStands() {
    #expect(AgentMap.facts(.starting, model: "") == "Working")
    #expect(AgentMap.facts(.failed, model: "claude-haiku-4-5") == "Failed · \(modelName("claude-haiku-4-5"))")
}

@Test("the main agent centres on the agents it spawned one column left")
func theMainAgentCentres() {
    let g = Graph.self
    let (at, size) = g.layout([nil, nil, nil])
    #expect(at[0] == (g.pad, g.pad + (g.height + g.vgap)))
    #expect(at[1...].allSatisfy { $0.x == g.pad + g.width + g.gap })
    #expect(size == (g.pad * 2 + 2 * g.width + g.gap, g.pad * 2 + 3 * (g.height + g.vgap) - g.vgap))
}

@Test("a nested agent sits a column right of its parent which centres on it")
func aNestedAgentSitsAColumnRight() {
    let g = Graph.self
    let (at, size) = g.layout([nil, 0, nil])
    #expect(at[2] == (g.pad + 2 * (g.width + g.gap), g.pad))
    #expect(at[1].y == at[2].y)
    #expect(at[3].y == g.pad + g.height + g.vgap)
    #expect(size.x == g.pad * 2 + 3 * g.width + 2 * g.gap)
}

@Test("an edge on one row is a straight line and one across rows bends in the gap")
func anEdgeOnOneRowIsStraight() {
    let straight = Graph.elbow((0, 10), (44, 10))
    #expect(straight.count == 2 && straight[0] == (0, 10) && straight[1] == (44, 10))
    let bent = Graph.elbow((0, 0), (44, 40))
    #expect(bent[0] == (0, 0) && bent.last! == (44, 40))
    #expect(bent.contains { $0 == (Graph.gap / 2, 8) } && bent.contains { $0 == (Graph.gap / 2, 32) })
    #expect(bent.allSatisfy { (0...44).contains($0.x) && (0...40).contains($0.y) })
}

@Test("dashes flow along the edge as the phase moves")
func dashesFlow() {
    let line: [Graph.Point] = [(0, 0), (20, 0)]
    let starts = { (phase: Double) in Graph.dashes(line, on: 3, off: 5, phase: phase).map { [$0[0].x, $0.last!.x] } }
    #expect(starts(0) == [[0, 3], [8, 11], [16, 19]])
    #expect(starts(2) == [[2, 5], [10, 13], [18, 20]])
}
