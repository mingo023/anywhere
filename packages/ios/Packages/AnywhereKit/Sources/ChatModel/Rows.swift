/// One list row; ids are stable across refreshes so the list keeps measured heights.
public enum Row: Hashable, Sendable {
    case prompt(turn: Int)
    /// The line heading a reply: who worked and for how long. It opens the reply's phases.
    case fold(turn: Int)
    /// A run of work: thinking and tool calls.
    case phase(turn: Int, blocks: Range<Int>)
    /// Sub-agents and commands that finished in the background, heading the turn the agent began itself to report on them.
    case notice(turn: Int)
    /// A `.notice` block: why the agent went on, heading the turn it set going, or an error it went on after, in place.
    case note(turn: Int, block: Int)
    /// The turn's latest todo list, as a card after its work.
    case tasks(turn: Int, block: Int)
    /// One markdown block (`range`, UTF-8 offsets into the message text) of a text or plan block.
    case text(turn: Int, block: Int, range: Range<Int>)
    /// An AskUserQuestion call: a marker that ends the phase before it.
    case question(turn: Int, block: Int)
    case compacted(turn: Int, block: Int)
    case end(turn: Int)
    /// What the agent is doing, under the reply it's at work on (`turn` holds its fold). Rebuilt with the trailer.
    case live(turn: Int)
    /// "Sending…", while a send is on its way.
    case working
    /// An outgoing message not yet in the transcript.
    case pending(Int)

    /// `{turn}`, `{turn}#notice`, `{turn}#fold`, `{turn}#p{first}`, `{turn}#{block}.{start}`, `{turn}#{block}`, `{turn}#end`, `{turn}#live`, `working`, `pending-{i}`.
    public func id(_ t: Transcript) -> String {
        switch self {
        case .prompt(let ix): t.turns[ix].id
        case .notice(let ix): "\(t.turns[ix].id)#notice"
        case .fold(let ix): "\(t.turns[ix].id)#fold"
        case .phase(let ix, let blocks): "\(t.turns[ix].id)#p\(blocks.lowerBound)"
        case .text(let ix, let block, let range): "\(t.turns[ix].id)#\(block).\(range.lowerBound)"
        case .tasks(let ix, let block), .question(let ix, let block), .compacted(let ix, let block), .note(let ix, let block):
            "\(t.turns[ix].id)#\(block)"
        case .end(let ix): "\(t.turns[ix].id)#end"
        case .live(let ix): "\(t.turns[ix].id)#live"
        case .working: "working"
        case .pending(let i): "pending-\(i)"
        }
    }

    public var turn: Int? {
        switch self {
        case .prompt(let t), .notice(let t), .note(let t, _), .fold(let t), .phase(let t, _), .tasks(let t, _), .text(let t, _, _),
            .question(let t, _), .compacted(let t, _), .end(let t):
            t
        case .live, .working, .pending: nil
        }
    }
}

/// The transcript's rows, then a row per pending send, "Sending…" while one is on its way, and what the agent is doing while at work.
public func rows(_ t: Transcript, pending: Int, sending: Bool) -> [Row] {
    var rows: [Row] = []
    rowsFrom(&rows, t, 0, pending: pending, sending: sending)
    return rows
}

/// Appends the rows of `t`'s turns from `from` on, then the trailer.
/// A turn lays out as its prompt or what it reports, the notices that set it going, the fold if it starts a reply, its
/// phases and the text between them, its todos, then its answer.
func rowsFrom(_ rows: inout [Row], _ t: Transcript, _ from: Int, pending: Int, sending: Bool) {
    for ix in from..<max(from, t.turns.count) {
        let turn = t.turns[ix]
        if turn.prompt != nil { rows.append(.prompt(turn: ix)) }
        if !reported(t, ix).isEmpty { rows.append(.notice(turn: ix)) }
        let lead = turn.blocks.prefix { if case .notice = $0 { true } else { false } }.count
        rows.append(contentsOf: (0..<lead).map { .note(turn: ix, block: $0) })
        // A turn stopped or failed before anything came has nothing to fold.
        let failedEmpty = turn.blocks.isEmpty && { if case .failed? = turn.end { true } else { false } }()
        if foldsAt(t, ix) && !failedEmpty { rows.append(.fold(turn: ix)) }
        // Text after the turn's last step is the answer.
        let work = workEnd(turn)
        var phase: Int?
        if lead < work {
            for b in lead..<work {
                let block = turn.blocks[b]
                if isWork(block) {
                    if phase == nil { phase = b }
                } else if case .todos = block {
                } else {
                    flush(&rows, ix, &phase, b)
                    plain(&rows, ix, b, block)
                }
            }
        }
        flush(&rows, ix, &phase, work)
        if let b = turn.blocks.lastIndex(where: { if case .todos = $0 { true } else { false } }) {
            rows.append(.tasks(turn: ix, block: b))
        }
        for b in max(work, lead)..<max(work, lead, turn.blocks.count) {
            plain(&rows, ix, b, turn.blocks[b])
        }
        if turn.end != nil { rows.append(.end(turn: ix)) }
    }
    rows.append(contentsOf: (0..<pending).map(Row.pending))
    if sending { rows.append(.working) }
    if let fold = liveTail(t) { rows.append(.live(turn: fold)) }
}

/// A block's own rows outside any phase; todos have their place after the work.
private func plain(_ rows: inout [Row], _ turn: Int, _ b: Int, _ block: Block) {
    switch block {
    case .text(let m), .plan(let m):
        rows.append(contentsOf: markdownBlocks(m.text).map { .text(turn: turn, block: b, range: $0) })
    case .tool(let c):
        if case .question = c.kind { rows.append(.question(turn: turn, block: b)) }
    case .compacted: rows.append(.compacted(turn: turn, block: b))
    case .notice: rows.append(.note(turn: turn, block: b))
    default: break
    }
}

private func flush(_ rows: inout [Row], _ turn: Int, _ phase: inout Int?, _ end: Int) {
    if let start = phase {
        rows.append(.phase(turn: turn, blocks: start..<end))
        phase = nil
    }
}

/// A step of a phase: thinking, a tool call or a sub-agent, but not a question, which stands apart.
public func isWork(_ b: Block) -> Bool {
    switch b {
    case .thinking: return true
    case .tool(let c):
        if case .question = c.kind { return false }
        return true
    default: return false
    }
}

func workEnd(_ turn: Turn) -> Int {
    turn.blocks.lastIndex(where: isWork).map { $0 + 1 } ?? 0
}

/// Whether block `block` of `turn` is text the agent wrote between its steps rather than its answer.
public func narrates(_ turn: Turn, _ block: Int) -> Bool {
    guard block < workEnd(turn), block < turn.blocks.count, case .text = turn.blocks[block] else { return false }
    return true
}

private func isCompacted(_ b: Block) -> Bool {
    if case .compacted = b { true } else { false }
}

func compaction(_ turn: Turn) -> Bool {
    turn.prompt == nil && !turn.blocks.isEmpty && turn.blocks.allSatisfy(isCompacted)
}

/// Whether turn `ix` heads a reply with a fold: a prompt's turn, unless it steered the one before, or a turn the agent
/// begins itself, as when a hook or a background task sets it going, other than a compaction.
func foldsAt(_ t: Transcript, _ ix: Int) -> Bool {
    let turn = t.turns[ix]
    return turn.prompt != nil ? !steered(t, ix) : !compaction(turn)
}

/// Whether turn `ix`'s prompt came while the agent was still on the turn before, which it carries on: that turn never
/// ends, as the agent ends and times the two as one.
func steered(_ t: Transcript, _ ix: Int) -> Bool {
    t.turns[ix].prompt != nil && ix > 0 && working(t.turns[ix - 1])
}

/// The turn whose fold heads the reply `turn` belongs to.
public func foldOf(_ t: Transcript, _ turn: Int) -> Int? {
    guard !t.turns.isEmpty else { return nil }
    return (0...min(turn, t.turns.count - 1)).reversed().first { foldsAt(t, $0) }
}

/// The turns of the reply whose fold sits on `turn`.
public func folded(_ t: Transcript, _ turn: Int) -> Range<Int> {
    let end = (turn + 1..<max(turn + 1, t.turns.count)).first { foldsAt(t, $0) || compaction(t.turns[$0]) } ?? t.turns.count
    return turn..<end
}

/// Whether the agent is still at work on `turns`: the last of them is open, or a sub-agent one started runs on.
public func atWorkOn(_ t: Transcript, _ turns: Range<Int>) -> Bool {
    let open = turns.upperBound == t.turns.count && turns.upperBound > turns.lowerBound && working(t.turns[turns.upperBound - 1])
    return open || t.turns[turns].contains { $0.blocks.contains(where: runningAgent) }
}

/// The fold of the last reply while the agent is at work on it, or on a sub-agent of any reply in the background; the
/// live line sits under the newest exchange rather than on a fold.
public func liveTail(_ t: Transcript) -> Int? {
    guard !t.turns.isEmpty, let fold = foldOf(t, t.turns.count - 1) else { return nil }
    return atWorkOn(t, 0..<t.turns.count) ? fold : nil
}

/// The turn whose fold held the live line before, `before`, once `t`'s no longer does: its reply just settled.
public func settledTail(_ before: String?, _ t: Transcript) -> String? {
    guard let before else { return nil }
    return liveTail(t).map { t.turns[$0].id } != before ? before : nil
}

/// Sub-agents and commands still running in `turns` after their own turn ended, the ones working in the background, with
/// their turn and block.
public func inBackground(_ turns: ArraySlice<Turn>) -> [(turn: Int, block: Int, call: Call)] {
    var out: [(Int, Int, Call)] = []
    for (t, turn) in zip(turns.indices, turns) where turn.end != nil {
        for (b, block) in turn.blocks.enumerated() {
            guard case .tool(let c) = block, c.status == .running else { continue }
            switch c.kind {
            case .agent, .shell: out.append((t - turns.startIndex, b, c))
            default: break
            }
        }
    }
    return out
}

/// The prompt row a step up or down lands on from the top row `top`: up goes to the start of the exchange showing, or
/// the one before if already there.
public func stepPrompt(_ rows: [Row], _ top: Int, down: Bool) -> Int? {
    let prompts = rows.indices.filter { if case .prompt = rows[$0] { true } else { false } }
    return down ? prompts.first { $0 > top } : prompts.last { $0 < top }
}

/// The row that shows block `block` of turn `turn`: the phase holding it.
public func rowOf(_ rows: [Row], turn: Int, block: Int) -> Int? {
    rows.firstIndex { if case .phase(turn, let blocks) = $0 { blocks.contains(block) } else { false } }
}

/// The sub-agents and commands turn `ix` reports on: it's one the agent began itself, and they finished after their own
/// turn ended, since the turn before `ix` began and by the time `ix` did.
public func reported(_ t: Transcript, _ ix: Int) -> [Call] {
    let turn = t.turns[ix]
    guard ix > 0, let at = start(turn), let since = start(t.turns[ix - 1]) else { return [] }
    if turn.prompt != nil || compaction(turn) { return [] }
    var out: [Call] = []
    for own in t.turns[..<ix] {
        guard let end = endedAt(own) else { continue }
        for b in own.blocks {
            guard case .tool(let c) = b, c.status != .running, let ms = c.durationMs else { continue }
            switch c.kind {
            case .agent, .shell:
                let done = c.at + ms
                if done > end && done > since && done <= at { out.append(c) }
            default: break
            }
        }
    }
    return out
}

/// When the agent began `turn`: its prompt, else its first timed block.
func start(_ turn: Turn) -> Int? {
    turn.prompt?.at ?? turn.blocks.lazy.compactMap(blockAt).first
}

func endedAt(_ turn: Turn) -> Int? {
    guard let end = turn.end, let s = start(turn) else { return nil }
    return s + end.durationMs
}

/// Whether `b` is a sub-agent still at work.
public func runningAgent(_ b: Block) -> Bool {
    guard case .tool(let c) = b, c.status == .running, case .agent = c.kind else { return false }
    return true
}

/// The turns of the reply `turn` closes, or nil while a later turn carries it on. A reply is a turn and the steers
/// sent into it.
public func reply(_ t: Transcript, _ turn: Int) -> Range<Int>? {
    if turn + 1 < t.turns.count && steered(t, turn + 1) { return nil }
    guard let start = (0...turn).reversed().first(where: { $0 == 0 || !steered(t, $0) }) else { return nil }
    return start..<turn + 1
}

/// What a reply that ended well took, summed over its turns; nil when its last turn failed or is still open.
public func worked(_ t: Transcript, _ turns: Range<Int>) -> (ms: Int, usage: Usage?)? {
    guard let last = t.turns[turns].last, case .done? = last.end else { return nil }
    var ms = 0
    var sum: Usage?
    for turn in t.turns[turns] {
        guard case .done(let d, let u)? = turn.end else { continue }
        ms += d
        if let a = sum, let b = u {
            sum = Usage(input: a.input + b.input, output: a.output + b.output, cacheRead: a.cacheRead + b.cacheRead)
        } else {
            sum = sum ?? u
        }
    }
    return (ms, sum)
}

/// Whether the agent is on a turn.
public func atWork(_ t: Transcript) -> Bool {
    t.turns.last.map(working) ?? false
}

/// An open turn holding only a compaction is the agent tidying up between prompts, not working.
func working(_ turn: Turn) -> Bool {
    turn.end == nil && (turn.prompt != nil || !turn.blocks.allSatisfy(isCompacted))
}

/// 16 over the first row and 14 at each exchange; within one, 6 between parts, 10 between markdown blocks of one
/// message, and none above a phase, whose rail runs on from the fold, or "Sending…", which brings its own.
public func topGap(_ prev: Row?, _ row: Row) -> Double {
    guard let prev else { return 16 }
    switch (prev, row) {
    case (_, .prompt), (_, .pending), (.pending, .live): return 14
    case (_, .phase), (_, .working): return 0
    case (.text(let a, let x, _), .text(let b, let y, _)) where a == b && x == y: return 10
    case (_, .tasks): return 12
    case (_, .question), (_, .compacted), (_, .notice), (_, .note): return 8
    default: return 6
    }
}

/// A phase's headline: what the work did, as "Edited a.rs · Ran 2 commands", in the present tense while `live`;
/// sub-agents go by their own state, as "Using a subagent".
public func phaseTitle(_ blocks: ArraySlice<Block>, live: Bool) -> String {
    let calls = blocks.compactMap(\.call)
    var edited: [String] = []
    for c in calls {
        switch c.kind {
        case .edit(let path, _), .write(let path, _): if !edited.contains(path) { edited.append(path) }
        default: break
        }
    }
    let explored = calls.contains {
        switch $0.kind {
        case .read, .search, .fetch: true
        default: false
        }
    }
    let ran = calls.filter { if case .shell = $0.kind { true } else { false } }.count
    var parts: [String] = []
    let agents = calls.filter { $0.agent != nil }
    let using = agents.contains { $0.status == .running } ? "Using" : "Used"
    switch agents.count {
    case 0: break
    case 1: parts.append("\(using) a subagent")
    case let n: parts.append("\(using) \(n) subagents")
    }
    switch edited.count {
    case 0: break
    case 1: parts.append("\(live ? "Editing" : "Edited") \(edited[0].split(separator: "/", omittingEmptySubsequences: false).last.map(String.init) ?? edited[0])")
    case let n: parts.append("\(live ? "Editing" : "Edited") \(n) files")
    }
    if explored { parts.append(live ? "Exploring the project" : "Explored the project") }
    switch ran {
    case 0: break
    case 1: parts.append(live ? "Executing a command" : "Ran a command")
    case let n: parts.append("\(live ? "Executing" : "Ran") \(n) commands")
    }
    if parts.isEmpty && blocks.allSatisfy({ if case .thinking = $0 { true } else { false } }) { return live ? "Thinking" : "Thought" }
    return parts.isEmpty ? "Working" : parts.joined(separator: " · ")
}

/// The range of `old` to replace and how many new rows replace it, sharing the longest equal prefix.
public func splice(_ old: [String], _ new: [String]) -> (range: Range<Int>, count: Int) {
    var shared = 0
    while shared < old.count, shared < new.count, old[shared] == new[shared] { shared += 1 }
    return (shared..<old.count, new.count - shared)
}

/// UTF-8 ranges of top-level markdown blocks, never splitting a fenced code block.
/// Blank lines end a block unless a list carries on past them.
public func markdownBlocks(_ text: String) -> [Range<Int>] {
    var text = text
    return text.withUTF8 { markdownBlocks(Array($0)) }
}

func markdownBlocks(_ bytes: [UInt8]) -> [Range<Int>] {
    var blocks: [Range<Int>] = []
    var current: Range<Int>?
    var fence: (UInt8, Int)?
    var list = false
    var afterBlank = false
    var start = 0
    while start < bytes.count {
        var next = start
        while next < bytes.count && bytes[next] != 0x0A { next += 1 }
        var end = next
        while end > start && (bytes[end - 1] == 0x0A || bytes[end - 1] == 0x0D) { end -= 1 }
        let range = start..<end
        start = next < bytes.count ? next + 1 : next
        let line = bytes[range]
        if let (ch, len) = fence {
            current = current!.lowerBound..<range.upperBound
            if closesFence(line, ch, len) {
                fence = nil
                blocks.append(current!)
                current = nil
            }
            continue
        }
        if blankLine(line) {
            afterBlank = true
            continue
        }
        let item = listItem(line)
        let indented = line.starts(with: [0x20, 0x20]) || line.first == 0x09
        fence = opensFence(line)
        let isHeading = heading(line)
        let continues = !afterBlank || (list && (item || indented))
        if let block = current, continues && fence == nil && !isHeading {
            current = block.lowerBound..<range.upperBound
            list = list || item
        } else {
            if let block = current { blocks.append(block) }
            current = range
            list = item
        }
        if isHeading, let block = current {
            blocks.append(block)
            current = nil
        }
        afterBlank = false
    }
    if let current { blocks.append(current) }
    return blocks
}

private func isSpace(_ c: UInt8) -> Bool { c == 0x20 || c == 0x09 || c == 0x0A || c == 0x0D || c == 0x0B || c == 0x0C }

private func blankLine(_ line: ArraySlice<UInt8>) -> Bool {
    if line.allSatisfy(isSpace) { return true }
    guard line.contains(where: { $0 >= 0x80 }) else { return false }
    return String(decoding: line, as: UTF8.self).allSatisfy(\.isWhitespace)
}

private func indent(_ line: ArraySlice<UInt8>) -> ArraySlice<UInt8>? {
    let rest = line.drop { $0 == 0x20 }
    return line.count - rest.count <= 3 ? rest : nil
}

private func opensFence(_ line: ArraySlice<UInt8>) -> (UInt8, Int)? {
    guard let rest = indent(line), let ch = rest.first, ch == UInt8(ascii: "`") || ch == UInt8(ascii: "~") else { return nil }
    let len = rest.prefix { $0 == ch }.count
    guard len >= 3, !(ch == UInt8(ascii: "`") && rest.dropFirst(len).contains(ch)) else { return nil }
    return (ch, len)
}

private func closesFence(_ line: ArraySlice<UInt8>, _ ch: UInt8, _ len: Int) -> Bool {
    guard let rest = indent(line) else { return false }
    let run = rest.prefix { $0 == ch }.count
    return run >= len && blankLine(rest.dropFirst(run))
}

private func heading(_ line: ArraySlice<UInt8>) -> Bool {
    guard let rest = indent(line) else { return false }
    let hashes = rest.prefix { $0 == UInt8(ascii: "#") }.count
    guard (1...6).contains(hashes) else { return false }
    let after = rest.dropFirst(hashes).first
    return after == nil || after == 0x20 || after == 0x09
}

private func listItem(_ line: ArraySlice<UInt8>) -> Bool {
    let rest = line.drop(while: isSpace)
    let digits = rest.prefix { $0 >= 0x30 && $0 <= 0x39 }.count
    var marker = 0
    if digits == 0 {
        if let c = rest.first, c == UInt8(ascii: "-") || c == UInt8(ascii: "*") || c == UInt8(ascii: "+") { marker = 1 }
    } else if digits <= 9, let c = rest.dropFirst(digits).first, c == UInt8(ascii: ".") || c == UInt8(ascii: ")") {
        marker = digits + 1
    }
    guard marker > 0 else { return false }
    let after = rest.dropFirst(marker).first
    return after == nil || after == 0x20 || after == 0x09
}
