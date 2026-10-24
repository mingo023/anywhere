import Domain

/// The map of who spawned whom: its chip, its nodes' words, where it stands and how its graph lays out.
public enum AgentMap {
    /// How long a closed map stays drawn while it fades.
    public static let shutting: Seconds = 0.5

    /// Where the map stands.
    public struct Nav: Hashable, Sendable {
        /// The agent whose map shows.
        public private(set) var open: String?
        /// A map closed a moment ago, drawn while it fades.
        var closing: (agent: String, at: Seconds)?
        /// The node picked: 0 is the main agent, then `spawned`'s order.
        public private(set) var selected = 0
        /// The call whose details show over the graph.
        public private(set) var detail: String?
        /// The details last shown, kept while they slide back out.
        public private(set) var shown: String?
        public var promptOpen = false

        public init() {}

        public static func == (a: Nav, b: Nav) -> Bool {
            a.open == b.open && a.closing?.agent == b.closing?.agent && a.closing?.at == b.closing?.at && a.selected == b.selected
                && a.detail == b.detail && a.shown == b.shown && a.promptOpen == b.promptOpen
        }

        public func hash(into h: inout Hasher) {
            h.combine(open)
            h.combine(selected)
            h.combine(detail)
        }

        public func isOpen(_ agent: String) -> Bool { open == agent }

        /// Whether `agent`'s map is drawn: open, or still fading out.
        public func drawn(_ agent: String, _ now: Seconds) -> Bool {
            isOpen(agent) || closing.map { $0.agent == agent && now - $0.at < shutting } ?? false
        }

        /// Opens `agent`'s map on its graph, or closes it when it's open.
        public mutating func toggle(_ agent: String, _ now: Seconds) {
            if isOpen(agent) { return close(now) }
            self = Nav()
            open = agent
        }

        public mutating func close(_ now: Seconds) {
            if let agent = open {
                open = nil
                closing = (agent, now)
            }
        }

        public mutating func show(_ node: Int, _ call: String) {
            selected = node
            detail = call
            shown = call
            promptOpen = false
        }

        public mutating func back() { detail = nil }

        /// Esc leaves the details for the graph, and the graph for the chat.
        public mutating func escape(_ now: Seconds) {
            if detail != nil { back() } else { close(now) }
        }

        /// Moves the pick down or up the `count` nodes, stopping at either end.
        public mutating func step(down: Bool, _ count: Int) {
            selected = down ? min(selected + 1, max(count - 1, 0)) : max(selected - 1, 0)
        }

        public enum Key: Hashable, Sendable {
            case escape, up, down, enter, right, left
        }

        /// Takes `key` on the map whose sub-agents' calls are `calls`; false for a key it leaves alone. A pick the map no
        /// longer holds, as after a new turn spawned fewer, opens nothing.
        public mutating func key(_ key: Key, _ calls: [String], _ now: Seconds) -> Bool {
            switch key {
            case .escape: escape(now)
            case .down where detail == nil, .up where detail == nil: step(down: key == .down, calls.count + 1)
            case .enter where detail == nil, .right where detail == nil:
                guard selected > 0, selected - 1 < calls.count else { return false }
                show(selected, calls[selected - 1])
            case .left where detail != nil: back()
            default: return false
            }
            return true
        }
    }

    /// How long `call` has run, in ms: until now while it runs.
    public static func ranMs(_ call: Call, _ now: Int) -> Int {
        call.status == .running ? now - call.at : call.durationMs ?? 0
    }

    /// "32.9k", "640".
    static func short(_ n: Int) -> String {
        n < 1000 ? "\(n)" : String(format: "%.1fk", Double(n) / 1000)
    }

    /// "32.9k tokens", "640 tokens".
    public static func tokens(_ n: Int) -> String { "\(short(n)) tokens" }

    /// A sub-agent's second line: how long it ran and the context it used, or how long it ran before it failed.
    public static func meta(_ call: Call, tokens used: Int, now: Int) -> String {
        let ran = TimeText.elapsed(seconds: ranMs(call, now) / 1000)
        if call.status == .failed { return "Failed after \(ran)" }
        return used == 0 ? ran : "\(ran) · \(tokens(used))"
    }

    /// What the main agent is on: the sub-agents its turn ends handing off to, its newest step, or done but for those still at work.
    public static func mainLine(_ t: Transcript, _ map: [Spawned]) -> String {
        let still = map.contains { $0.call.status == .running }
        guard let turn = t.turns.last, atWork(t) else { return still ? "Turn ended · agents still working" : "Done" }
        if let last = turn.blocks.last, runningAgent(last) {
            let n = turn.blocks.filter(runningAgent).count
            return "Handed off to \(n) agent\(n == 1 ? "" : "s")"
        }
        return turn.blocks.reversed().lazy.compactMap(\.call).first.map(Agent.step) ?? "Working"
    }

    /// The chip's words, and whether any agent still works.
    public static func chipLabel(_ map: [Spawned]) -> (live: Bool, label: String) {
        let running = map.filter { $0.call.status == .running }.count
        let n = map.count
        let agents = n == 1 ? "agent" : "agents"
        switch running {
        case 0: return (false, "\(n) \(agents)")
        case n: return (true, "\(n) \(agents)")
        default: return (true, "\(running) of \(n) \(agents)")
        }
    }

    /// "4 agents · 2 working · 1 failed".
    public static func summary(_ map: [Spawned]) -> String {
        let working = map.filter { $0.call.status == .running }.count
        let failed = map.filter { $0.call.status == .failed }.count
        var parts = ["\(map.count) agent\(map.count == 1 ? "" : "s")"]
        if working > 0 { parts.append("\(working) working") }
        if failed > 0 { parts.append("\(failed) failed") }
        return parts.joined(separator: " · ")
    }

    /// The turn and block of the main agent's call that `map[ix]` was spawned under, to show it in the chat.
    public static func place(_ t: Transcript, _ map: [Spawned], _ ix: Int) -> (turn: Int, block: Int)? {
        var ix = ix
        while let parent = map[ix].parent { ix = parent }
        return findCall(t, map[ix].call.id)
    }

    /// A step's tool and what it acted on: "Read" and a path, or the tool's own name and its command.
    public static func step(_ call: Call) -> (tool: String, arg: String) {
        let (verb, arg) = toolLine(call)
        return (verb.isEmpty ? call.name : verb, arg.string)
    }

    /// "Working · Haiku 5.5", what the sub-agent is and where it stands.
    public static func facts(_ mark: Mark, model: String) -> String {
        let state =
            switch mark {
            case .starting, .running: "Working"
            case .done: "Done"
            case .failed: "Failed"
            }
        return model.isEmpty ? state : "\(state) · \(modelName(model))"
    }
}

/// How the agent map's graph lays out: one column per generation, a parent centred on its children.
public enum Graph {
    public typealias Point = (x: Double, y: Double)

    public static let width: Double = 232
    public static let height: Double = 64
    public static let gap: Double = 44
    public static let vgap: Double = 10
    public static let pad: Double = 16
    public static let dash: Double = 3
    public static let dashGap: Double = 5

    /// Each node's top left and the graph's size. Node 0 is the main agent and node `i + 1` the sub-agent whose parent
    /// is `parents[i]`, an index into the same list, or the main agent. The leaves stack in order.
    public static func layout(_ parents: [Int?]) -> (at: [Point], size: Point) {
        let n = parents.count + 1
        var kids = Array(repeating: [Int](), count: n)
        var depth = Array(repeating: 0, count: n)
        for (i, p) in parents.enumerated() {
            let p = p.map { $0 + 1 } ?? 0
            kids[p].append(i + 1)
            depth[i + 1] = depth[p] + 1
        }
        var at = Array(repeating: (x: 0.0, y: 0.0), count: n)
        var slot = 0
        func place(_ node: Int) {
            let x = pad + Double(depth[node]) * (width + gap)
            let y: Double
            if let first = kids[node].first, let last = kids[node].last {
                kids[node].forEach(place)
                y = (at[first].y + at[last].y) / 2
            } else {
                slot += 1
                y = pad + Double(slot - 1) * (height + vgap)
            }
            at[node] = (x, y)
        }
        place(0)
        let columns = Double((depth.max() ?? 0) + 1)
        return (at, (pad * 2 + columns * width + (columns - 1) * gap, pad * 2 + Double(max(slot, 1)) * (height + vgap) - vgap))
    }

    /// The edge from a parent's right side to a child's left: across, down the middle of the gap with rounded bends, and
    /// across again, as points close enough to draw the bends as lines.
    public static func elbow(_ from: Point, _ to: Point) -> [Point] {
        let dy = to.y - from.y
        if abs(dy) < 0.5 { return [from, to] }
        let mid = from.x + gap / 2
        let r = min(8, abs(dy) / 2)
        let dir: Double = dy < 0 ? -1 : 1
        func bend(_ a: Point, _ corner: Point, _ b: Point) -> [Point] {
            (1...4).map { i in
                let t = Double(i) / 4
                let u = 1 - t
                return (u * u * a.x + 2 * u * t * corner.x + t * t * b.x, u * u * a.y + 2 * u * t * corner.y + t * t * b.y)
            }
        }
        var points: [Point] = [from, (mid - r, from.y)]
        points += bend((mid - r, from.y), (mid, from.y), (mid, from.y + dir * r))
        points.append((mid, to.y - dir * r))
        points += bend((mid, to.y - dir * r), (mid, to.y), (mid + r, to.y))
        points.append(to)
        return points
    }

    static func distance(_ p: Point, _ q: Point) -> Double {
        let (dx, dy) = (q.x - p.x, q.y - p.y)
        return (dx * dx + dy * dy).squareRoot()
    }

    /// The stretch of `points` from distance `a` to `b` along it.
    static func between(_ points: [Point], _ a: Double, _ b: Double) -> [Point] {
        var out: [Point] = []
        var walked = 0.0
        for (p, q) in zip(points, points.dropFirst()) {
            let len = distance(p, q)
            func at(_ d: Double) -> Point {
                let t = len == 0 ? 0 : min(max((d - walked) / len, 0), 1)
                return (p.x + (q.x - p.x) * t, p.y + (q.y - p.y) * t)
            }
            if walked + len >= a && walked <= b {
                if out.isEmpty { out.append(at(a)) }
                out.append(at(min(b, walked + len)))
            }
            walked += len
        }
        return out
    }

    /// `points` cut into dashes `on` long and `off` apart, the pattern moved `phase` along so it flows from parent to child.
    public static func dashes(_ points: [Point], on: Double = dash, off: Double = dashGap, phase: Double) -> [[Point]] {
        let total = zip(points, points.dropFirst()).map(distance).reduce(0, +)
        let period = on + off
        var start = phase.truncatingRemainder(dividingBy: period)
        if start < 0 { start += period }
        start -= period
        var out: [[Point]] = []
        while start < total {
            let (a, b) = (max(start, 0), min(start + on, total))
            if b > a { out.append(between(points, a, b)) }
            start += period
        }
        return out
    }
}
