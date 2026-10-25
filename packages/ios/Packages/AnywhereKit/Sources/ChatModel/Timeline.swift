import Wire

/// Folds pocketd timeline items, in seq order, into turns:
/// - `user` starts a turn, and `result` ends it.
/// - `assistant` → `.text`, `thinking` → `.thinking`, `tasks` → `.todos`, `plan` → `.plan`, `compact` → `.compacted`.
/// - `notice` → `.notice`: its `text`, else the `error` the agent went on after.
/// - `tool` → `.tool`, refreshed in place by `toolUseId`. A `task` detail is a sub-agent; WebFetch and WebSearch are fetch and search;
///   AskUserQuestion is a question.
/// - Items carrying `parentToolUseId` fold into that tool's sub-agent transcript.
public func fold(_ items: [TimelineItem]) -> Transcript {
    let ordered = sorted(items) ? items : items.sorted { $0.seq < $1.seq }
    var children: [String: [TimelineItem]] = [:]
    var top: [TimelineItem] = []
    for item in ordered {
        if let parent = parent(item) {
            children[parent, default: []].append(item)
        } else {
            top.append(item)
        }
    }
    return foldLevel(top, &children)
}

/// What the last fold of a timeline left, so the next refolds only the turns that can have changed.
public struct Fold: Sendable {
    /// The items last folded.
    var items: [TimelineItem] = []
    /// The index in `items` of the item that began each turn.
    var starts: [Int] = []
    /// Top-level calls by use id: (turn, block).
    var tools: [String: (Int, Int)] = [:]
    /// Sub-agent calls at any depth by use id, with the top-level turn holding them.
    var agents: [String: Int] = [:]
    /// The keys of `tools` and `agents` each turn added, so dropping turns drops only theirs.
    var added: [(tools: [String], agents: [String])] = []

    public init() {}

    /// Brings `t`, which holds this fold's last result, up to `fold(items)`, and returns the turns it replaced: `t`'s last ones.
    /// `unchanged` items at the front are known to be the ones last folded, so they aren't compared again.
    public mutating func refold(_ items: [TimelineItem], _ t: inout Transcript, unchanged: Int = 0) -> [Turn] {
        if !sorted(items) {
            self = Fold()
            let replaced = t.turns
            t = fold(items)
            return replaced
        }
        var same = min(unchanged, items.count, self.items.count)
        while same < items.count, same < self.items.count, items[same] == self.items[same] { same += 1 }
        if same == items.count && same == self.items.count { return [] }
        let from = resumable(items, same) ?? 0
        let start = from == 0 ? 0 : starts[from]
        let replaced = Array(t.turns[from...])
        t.turns.removeSubrange(from...)
        starts.removeSubrange(from...)
        for turn in added[min(from, added.count)...] {
            for key in turn.tools { tools[key] = nil }
            for key in turn.agents { agents[key] = nil }
        }
        added.removeSubrange(min(from, added.count)...)
        var children: [String: [TimelineItem]] = [:]
        for (i, item) in items.enumerated() {
            if let parent = parent(item) {
                // Kept turns already hold what unchanged items did to them.
                if agents[parent] == nil { children[parent, default: []].append(item) }
            } else if i >= start && !keptCall(item, from) {
                let turns = t.turns.count
                let key = step(&t, &tools, item)
                if t.turns.count > turns { starts.append(i) }
                while added.count < t.turns.count { added.append(([], [])) }
                if let key { added[t.turns.count - 1].tools.append(key) }
            }
        }
        claim(&t.turns, from: from, &children)
        while added.count < t.turns.count { added.append(([], [])) }
        for ix in from..<t.turns.count {
            recordAgents(t.turns[ix].blocks, ix, &agents, &added[ix].agents)
        }
        self.items.removeSubrange(same...)
        self.items.append(contentsOf: items[same...])
        return replaced
    }

    /// The turn holding item `same`, the first that changed, unless a changed item, old or new, refreshes a call
    /// or feeds a sub-agent in an earlier turn.
    func resumable(_ items: [TimelineItem], _ same: Int) -> Int? {
        var lo = 0
        var hi = starts.count
        while lo < hi {
            let mid = (lo + hi) / 2
            if starts[mid] < same { lo = mid + 1 } else { hi = mid }
        }
        guard lo > 0 else { return nil }
        let from = lo - 1
        func reachesBack(_ item: TimelineItem) -> Bool {
            if let parent = parent(item) { return agents[parent].map { $0 < from } ?? false }
            return keptCall(item, from)
        }
        if items[same...].contains(where: reachesBack) || self.items[same...].contains(where: reachesBack) { return nil }
        return from
    }

    /// Whether `item` refreshes a call in a turn before `from`.
    func keptCall(_ item: TimelineItem, _ from: Int) -> Bool {
        guard case .tool(let call) = item.body, let (turn, _) = tools[toolId(item, call)] else { return false }
        return turn < from
    }
}

func sorted(_ items: [TimelineItem]) -> Bool {
    var last = Int.min
    for item in items {
        if item.seq < last { return false }
        last = item.seq
    }
    return true
}

func parent(_ item: TimelineItem) -> String? {
    guard let p = item.parentToolUseId, !p.isEmpty else { return nil }
    return p
}

func foldLevel(_ items: [TimelineItem], _ children: inout [String: [TimelineItem]]) -> Transcript {
    var t = Transcript()
    var tools: [String: (Int, Int)] = [:]
    for item in items { step(&t, &tools, item) }
    claim(&t.turns, from: 0, &children)
    return t
}

/// Folds one item into `t`; `tools` finds each call (turn, block) by use id, so a later item refreshes it in place.
/// Returns the use id it added to `tools`, if any.
@discardableResult
func step(_ t: inout Transcript, _ tools: inout [String: (Int, Int)], _ item: TimelineItem) -> String? {
    switch item.body {
    case .user(let text):
        t.turns.append(Turn(id: item.id, prompt: Message(id: item.id, at: item.ts, text: text)))
    case .result(let ok, let durationMs, let error, let usage):
        open(&t, item)
        t.turns[t.turns.count - 1].end = ok
            ? .done(durationMs: max(durationMs, 0), usage: usage.map { Usage(input: $0.inputTokens, output: $0.outputTokens, cacheRead: $0.cacheReadTokens ?? 0) })
            : .failed(error: error ?? "", durationMs: max(durationMs, 0))
    case .tool(let wire):
        let id = toolId(item, wire)
        let block = Block.tool(call(id, item, wire))
        if let (turn, ix) = tools[id] {
            t.turns[turn].blocks[ix] = block
            return nil
        }
        open(&t, item)
        t.turns[t.turns.count - 1].blocks.append(block)
        tools[id] = (t.turns.count - 1, t.turns[t.turns.count - 1].blocks.count - 1)
        return id
    default:
        if let block = block(item) {
            open(&t, item)
            t.turns[t.turns.count - 1].blocks.append(block)
        }
    }
    return nil
}

func toolId(_ item: TimelineItem, _ call: Wire.ToolCall) -> String {
    call.toolUseId.isEmpty ? item.id : call.toolUseId
}

/// Each sub-agent call takes its items out of `children` as it folds them, so a parent cycle can't recurse forever.
func claim(_ turns: inout [Turn], from: Int, _ children: inout [String: [TimelineItem]]) {
    if children.isEmpty { return }
    for t in from..<turns.count {
        for b in turns[t].blocks.indices {
            guard case .tool(var call) = turns[t].blocks[b], case .agent(var sub) = call.kind,
                let kids = children.removeValue(forKey: call.id)
            else { continue }
            sub.transcript = foldLevel(kids, &children)
            call.kind = .agent(sub)
            turns[t].blocks[b] = .tool(call)
        }
    }
}

/// Notes each sub-agent call in `blocks`, at any depth and in the order `claim` reaches them, as held by top-level turn `turn`.
func recordAgents(_ blocks: [Block], _ turn: Int, _ agents: inout [String: Int], _ added: inout [String]) {
    for block in blocks {
        guard case .tool(let call) = block, case .agent(let sub) = call.kind else { continue }
        if agents[call.id] == nil {
            agents[call.id] = turn
            added.append(call.id)
        }
        for t in sub.transcript.turns { recordAgents(t.blocks, turn, &agents, &added) }
    }
}

/// Makes sure the last turn is still under way, starting one the agent began itself if not.
func open(_ t: inout Transcript, _ item: TimelineItem) {
    if t.turns.last.map({ $0.end != nil }) ?? true {
        t.turns.append(Turn(id: item.id))
    }
}

/// Nil for an item the chat doesn't show, a blank message among them.
func block(_ item: TimelineItem) -> Block? {
    func message(_ text: String) -> Message { Message(id: item.id, at: item.ts, text: text) }
    switch item.body {
    case .assistant(let text) where !blank(text): return .text(message(text))
    case .thinking(let text) where !blank(text): return .thinking(message(text))
    case .plan(let text) where !blank(text): return .plan(message(text))
    case .tasks(let tasks):
        return .todos(id: item.id, todos: tasks.map { Todo(text: $0.text, status: todoStatus($0.status)) })
    case .compact(let trigger): return .compacted(id: item.id, auto: trigger == "auto")
    case .notice(_, let error?) where !error.isEmpty: return .notice(message(error), error: true)
    case .notice(let text?, _) where !blank(text): return .notice(message(text), error: false)
    default: return nil
    }
}

func blank(_ s: String) -> Bool { s.allSatisfy(\.isWhitespace) }

func todoStatus(_ status: TaskItem.Status) -> Todo.Status {
    switch status {
    case .inProgress: .active
    case .completed: .done
    default: .pending
    }
}

func call(_ id: String, _ item: TimelineItem, _ c: Wire.ToolCall) -> Call {
    let name: String
    if c.name.isEmpty, case .other(let detailName, _, _) = c.detail { name = detailName } else { name = c.name }
    let status: CallStatus =
        switch c.status {
        case .ok: .ok
        case .error: .failed
        default: .running
        }
    return Call(id: id, name: name, kind: kind(name, c.detail), status: status, output: c.output, durationMs: c.durationMs, at: item.ts)
}

func kind(_ name: String, _ d: ToolDetail) -> ToolKind {
    switch d {
    case .shell(let command, _): return .shell(command: command)
    case .read(let path): return .read(path: path)
    case .edit(let path, let diff): return .edit(path: path, diff: diff)
    case .write(let path, let diff): return .write(path: path, diff: diff)
    case .search(let query, let path?) where !path.isEmpty: return .search(query: "\(query) in \(path)")
    case .search(let query, _): return .search(query: query)
    case .task(let description, let model, let tokens): return .agent(SubAgent(description: description, model: model ?? "", tokens: tokens ?? 0))
    default: break
    }
    let input: JSONValue
    let description: String
    if case .other(_, let i, let d) = d { (input, description) = (i, d ?? "") } else { (input, description) = (.null, "") }
    if name == "AskUserQuestion" { return .question(questions(input)) }
    if name == "WebFetch" { return .fetch(url: input["url"]?.string ?? "") }
    if name == "WebSearch" { return .search(query: input["query"]?.string ?? "") }
    if input == .null { return .other(input: "", description: description) }
    return .other(input: pretty(input), description: description)
}

/// AskUserQuestion's, from the tool's input; an input shaped otherwise has none.
func questions(_ input: JSONValue) -> [Question] {
    guard case .array(let list)? = input["questions"] else { return [] }
    var out: [Question] = []
    for q in list {
        guard case .object(let o) = q else { return [] }
        guard let question = text(o["question"]), let header = text(o["header"]) else { return [] }
        var multi = false
        switch o["multiSelect"] {
        case nil, .null?: break
        case .bool(let b)?: multi = b
        default: return []
        }
        var options: [Choice] = []
        switch o["options"] {
        case nil, .null?: break
        case .array(let list)?:
            for c in list {
                guard case .object(let co) = c, let label = text(co["label"]), let description = text(co["description"]) else { return [] }
                options.append(Choice(label: label, description: description))
            }
        default: return []
        }
        out.append(Question(header: header, question: question, options: options, multi: multi))
    }
    return out
}

/// A string field as serde's default reads it: "" when absent, nil when another type.
private func text(_ v: JSONValue?) -> String? {
    switch v {
    case nil, .null?: ""
    case .string(let s)?: s
    default: nil
    }
}

/// What the user is asked: the tool waiting on permission, or AskUserQuestion's questions.
public func ask(_ p: PermissionRequest) -> Ask {
    let k = kind(p.toolName, p.detail)
    var qs: [Question] = []
    if p.toolName == "AskUserQuestion", case .other(_, let input, _) = p.detail { qs = questions(input) }
    return Ask(requestId: p.requestId, tool: k, toolName: p.toolName, questions: qs)
}

extension JSONValue {
    subscript(key: String) -> JSONValue? {
        if case .object(let o) = self { o[key] } else { nil }
    }

    var string: String? {
        if case .string(let s) = self { s } else { nil }
    }
}

/// `value` as serde_json's pretty printer writes it: two-space indents, keys sorted.
func pretty(_ value: JSONValue) -> String {
    var out = ""
    write(value, 0, &out)
    return out
}

private func write(_ value: JSONValue, _ depth: Int, _ out: inout String) {
    let pad = String(repeating: "  ", count: depth + 1)
    switch value {
    case .null: out += "null"
    case .bool(let b): out += b ? "true" : "false"
    case .number(let n):
        if n.rounded() == n && abs(n) < 1e15 { out += String(Int(n)) } else { out += "\(n)" }
    case .string(let s): quote(s, &out)
    case .array(let list):
        if list.isEmpty {
            out += "[]"
            return
        }
        out += "[\n"
        for (i, v) in list.enumerated() {
            out += pad
            write(v, depth + 1, &out)
            out += i == list.count - 1 ? "\n" : ",\n"
        }
        out += String(repeating: "  ", count: depth) + "]"
    case .object(let o):
        if o.isEmpty {
            out += "{}"
            return
        }
        out += "{\n"
        let keys = o.keys.sorted()
        for (i, k) in keys.enumerated() {
            out += pad
            quote(k, &out)
            out += ": "
            write(o[k]!, depth + 1, &out)
            out += i == keys.count - 1 ? "\n" : ",\n"
        }
        out += String(repeating: "  ", count: depth) + "}"
    }
}

private func quote(_ s: String, _ out: inout String) {
    out += "\""
    for u in s.unicodeScalars {
        switch u {
        case "\"": out += "\\\""
        case "\\": out += "\\\\"
        case "\n": out += "\\n"
        case "\r": out += "\\r"
        case "\t": out += "\\t"
        case "\u{08}": out += "\\b"
        case "\u{0C}": out += "\\f"
        case _ where u.value < 0x20:
            let hex = String(u.value, radix: 16)
            out += "\\u" + String(repeating: "0", count: 4 - hex.count) + hex
        default: out.unicodeScalars.append(u)
        }
    }
    out += "\""
}
