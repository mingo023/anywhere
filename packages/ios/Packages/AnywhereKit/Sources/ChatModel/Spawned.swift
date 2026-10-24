/// A sub-agent on the agent map and the call that spawned it.
public struct Spawned: Hashable, Sendable {
    public var call: Call
    public var agent: SubAgent
    /// The index of the sub-agent that spawned it; nil when the main agent did.
    public var parent: Int?
}

/// The sub-agents of the latest turn that spawned any, and those of earlier turns still at work, each followed by the
/// ones it spawned.
public func spawned(_ t: Transcript) -> [Spawned] {
    let latest = t.turns.lastIndex { $0.blocks.contains { $0.call?.agent != nil } }
    var map: [Spawned] = []
    for (ix, turn) in t.turns.enumerated() {
        for call in turn.blocks.compactMap(\.call) {
            guard let agent = call.agent else { continue }
            if ix == latest || call.status == .running { push(&map, call, agent, nil) }
        }
    }
    return map
}

private func push(_ map: inout [Spawned], _ call: Call, _ agent: SubAgent, _ parent: Int?) {
    let me = map.count
    map.append(Spawned(call: call, agent: agent, parent: parent))
    for turn in agent.transcript.turns {
        for c in turn.blocks.compactMap(\.call) {
            if let a = c.agent { push(&map, c, a, me) }
        }
    }
}
