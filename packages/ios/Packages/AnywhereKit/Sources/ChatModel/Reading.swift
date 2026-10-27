import Domain
import Wire
import Foundation

/// What the chat's rows read: labels, counts and lines, worked out from the transcript alone.

/// The icon a row shows, named for what it means; the view picks the symbol.
public enum Glyph: Hashable, Sendable {
    case terminal, file, pencil, filePlus, search, globe, bot, blocks, bulb, tool
}

/// What a live fold says the agent is doing.
public enum Live: Hashable, Sendable {
    case stopping, thinking
    /// `background`: sub-agents and commands still running after the turn that began them ended.
    case working(secs: Int, background: Int)
    case approval, answer
}

/// The agent as the fold names it: "Claude", "Codex".
public func who(_ provider: String) -> String {
    switch provider {
    case "claude": "Claude"
    case "codex": "Codex"
    default: "Agent"
    }
}

/// "Opus 5.5 working for 7s · in the background"; the agent's name stands in for an unknown model.
public func liveLabel(_ who: String, _ model: String, _ live: Live) -> String {
    switch live {
    case .stopping: return "Stopping…"
    case .thinking: return "Thinking…"
    case .approval: return "Needs your approval"
    case .answer: return "Needs your answer"
    case .working(let secs, let background):
        let name = model.isEmpty ? who : model
        let rest =
            switch background {
            case 0: ""
            case 1: " · in the background"
            default: " · \(background) in the background"
            }
        return "\(name) working for \(TimeText.elapsed(seconds: secs))\(rest)"
    }
}

/// "Claude worked for 1m 4s".
public func settledLabel(_ who: String, _ ms: Int) -> String {
    "\(who) worked for \(TimeText.elapsed(seconds: ms / 1000))"
}

/// "claude-haiku-4-5-20251001" reads as "Haiku 4.5", "haiku" as "Haiku".
public func modelName(_ model: String) -> String {
    let bare = model.hasPrefix("claude-") ? String(model.dropFirst(7)) : model
    let words = bare.split(separator: "-", omittingEmptySubsequences: false).filter { $0.utf8.count < 8 }
    let family = words.first.map { $0.prefix(1).uppercased() + $0.dropFirst() } ?? ""
    let version = words.dropFirst()
    return version.isEmpty ? family : "\(family) \(version.joined(separator: "."))"
}

/// "claude-opus-5-5" reads as "Opus 5.5"; a missing model falls back to the provider.
public func modelLabel(model: String?, provider: String) -> String {
    guard let model, !model.isEmpty else { return provider.prefix(1).uppercased() + provider.dropFirst() }
    return modelName(model)
}

/// A detail block's lines, and how many more were cut.
public struct DetailLines: Hashable, Sendable {
    /// Lines a detail block shows before counting the rest.
    public static let max = 24
    /// Chars kept of one output line: lines clip, but each is still laid out whole.
    static let lineMaxChars = 8 * 1024

    public var lines: [String]
    public var more: Int

    /// Trims trailing blank lines and keeps the first 24.
    public static func head(_ lines: [String]) -> DetailLines {
        var lines = lines
        while let last = lines.last, last.allSatisfy(\.isWhitespace) { lines.removeLast() }
        let more = Swift.max(lines.count - max, 0)
        return DetailLines(lines: Array(lines.prefix(max)), more: more)
    }

    /// A tool's output, verbatim.
    public static func output(_ text: String) -> DetailLines {
        head(ChatModel.lines(text).map { $0.count > lineMaxChars ? String($0.prefix(lineMaxChars)) + "…" : String($0) })
    }

    /// "… 6 more lines" under the lines, when some were cut.
    public var moreLabel: String? { more > 0 ? "… \(more) more lines" : nil }
}

/// Which detail chips the user opened or shut, over each chip's default.
public struct Details: Hashable, Sendable {
    var chosen: [String: Bool] = [:]

    public init() {}

    public func open(_ key: String, _ fallback: Bool) -> Bool { chosen[key] ?? fallback }

    /// Flips the chip and returns whether it's now open.
    public mutating func toggle(_ key: String, _ fallback: Bool) -> Bool {
        let open = !open(key, fallback)
        chosen[key] = open
        return open
    }
}

/// `line` cut into chunks of `cols` chars.
func wrap(_ line: String, _ cols: Int) -> [String] {
    let chars = Array(line)
    if chars.count <= cols { return [line] }
    return stride(from: 0, to: chars.count, by: cols).map { String(chars[$0..<min($0 + cols, chars.count)]) }
}

/// Columns a thought wraps at, counted in chars so its height is known without layout.
let thoughtWrap = 96

/// A thought's lines, word-wrapped. Past the cap a live thought keeps its tail, the fresh thinking, and a settled one its head.
public func thinkingLines(_ text: String, live: Bool) -> DetailLines {
    var trimmed = Substring(text)
    while let last = trimmed.last, last.isWhitespace { trimmed = trimmed.dropLast() }
    var out = lines(String(trimmed)).flatMap { wrapWords(String($0)) }
    let more = out.count - DetailLines.max
    if live && more > 0 {
        out.removeFirst(more)
        // The cut can land on a paragraph break.
        while let first = out.first, first.allSatisfy(\.isWhitespace) { out.removeFirst() }
        return DetailLines(lines: out, more: more)
    }
    return .head(out)
}

/// `line` word-wrapped at 96 columns; a word longer than that is cut.
private func wrapWords(_ line: String) -> [String] {
    var out = [""]
    for word in line.split(whereSeparator: \.isWhitespace) {
        for piece in wrap(String(word), thoughtWrap) {
            let len = out[out.count - 1].count
            if len == 0 {
                out[out.count - 1] += piece
            } else if len + 1 + piece.count <= thoughtWrap {
                out[out.count - 1] += " " + piece
            } else {
                out.append(piece)
            }
        }
    }
    return out
}

/// How many todos are done, and how many there are.
public func todoCount(_ todos: [Todo]) -> (done: Int, total: Int) {
    (todos.filter { $0.status == .done }.count, todos.count)
}

/// "2 of 5", or "Complete" once all are done.
public func todoProgress(_ todos: [Todo]) -> String {
    let (done, n) = todoCount(todos)
    return done == n ? "Complete" : "\(done) of \(n)"
}

/// Whether a failed turn was the user stopping it rather than an error.
public func interrupted(_ error: String) -> Bool {
    error.lowercased().contains("interrupt")
}

/// "Interrupted · What should Claude do instead?"
public func interruptedNotice(_ who: String) -> String {
    "Interrupted · What should \(who) do instead?"
}

/// "Context compacted", or "… automatically".
public func compactedLabel(auto: Bool) -> String {
    auto ? "Context compacted automatically" : "Context compacted"
}

private func oneLine(_ s: String) -> String { s.split(whereSeparator: \.isWhitespace).joined(separator: " ") }

/// What a question marker reads: the first question's header once answered, else that it waits.
public func questionValue(_ call: Call) -> String {
    if call.status == .running { return "Awaiting your answer…" }
    guard case .question(let qs) = call.kind, let q = qs.first else { return "" }
    return q.header.allSatisfy(\.isWhitespace) ? oneLine(q.question) : oneLine(q.header)
}

/// The tool's icon, its verb and what it acts on, as a permission card names them.
public func permissionSubject(_ tool: ToolKind, _ toolName: String) -> (glyph: Glyph, verb: String, what: String) {
    switch tool {
    case .shell(let command): (.terminal, "Run", command)
    case .read(let path): (.file, "Read", path)
    case .edit(let path, _): (.pencil, "Edit", path)
    case .write(let path, _): (.filePlus, "Write", path)
    case .search(let query): (.search, "Search", query)
    case .fetch(let url): (.globe, "Fetch", url)
    case .agent(let a): (.bot, "Start agent", a.description)
    case .shared(let path, _): (.file, "Share", path)
    case .question, .other: (.blocks, "Use", toolName)
    }
}

/// What a tool row reads after its verb.
public enum Arg: Hashable, Sendable {
    /// A file, as a chip with its language's badge.
    case file(String)
    /// What was looked for or fetched, as plain text.
    case text(String)
    /// The call itself in the code font: a command's first line, or the tool's name.
    case raw(String)

    public var string: String {
        switch self {
        case .file(let s), .text(let s), .raw(let s): s
        }
    }
}

/// A call as one line: "Read [a.rs]", "Find fn group", or the command it ran.
public func toolLine(_ call: Call) -> (verb: String, arg: Arg) {
    switch call.kind {
    case .read(let path): return ("Read", .file(path))
    case .edit(let path, _): return ("Edit", .file(path))
    case .write(let path, _): return ("Create", .file(path))
    case .search(let query): return ("Find", .text(oneLine(query)))
    case .fetch(let url): return ("Fetch", .text(url))
    // pocketd names the command, or the task, a call on a background task acts on.
    case .shell(let command):
        let first = lines(command).map { $0.trimmingWhitespace() }.first { !$0.isEmpty } ?? "Shell"
        return (verb(call.name), .raw(first))
    case .other(_, let description) where !description.isEmpty: return (verb(call.name), .text(oneLine(description)))
    case .other(let input, _) where call.name == "Skill": return ("Skill", .text(skill(input).map { "/\($0)" } ?? ""))
    case .other: return ("", .raw(mcpName(call.name) ?? call.name))
    case .agent(let a): return ("", .raw(oneLine(a.description)))
    case .shared(let path, _): return ("Share", .file(path))
    case .question: return ("", .raw(call.name))
    }
}

private func verb(_ name: String) -> String {
    switch name {
    case "TaskStop", "KillShell": "Stop"
    case "TaskOutput", "BashOutput": "Read output of"
    case "Monitor": "Watch"
    default: ""
    }
}

private func skill(_ input: String) -> String? {
    let object = try? JSONSerialization.jsonObject(with: Data(input.utf8)) as? [String: Any]
    return object?["skill"] as? String
}

/// `mcp__linear__create_issue` as "linear · create_issue".
func mcpName(_ name: String) -> String? {
    guard name.hasPrefix("mcp__") else { return nil }
    let rest = name.dropFirst(5)
    guard let split = rest.range(of: "__") else { return nil }
    return "\(rest[..<split.lowerBound]) · \(rest[split.upperBound...])"
}

/// An edit's diff, when it has lines to open.
public func editDiff(_ call: Call) -> (path: String, diff: FileDiff)? {
    switch call.kind {
    case .edit(let path, let d?), .write(let path, let d?): d.lines.isEmpty ? nil : (path, d)
    default: nil
    }
}

/// Where a sub-agent stands, as its mark shows it.
public enum Mark: Hashable, Sendable {
    /// Running, nothing heard from it yet.
    case starting
    case running, done, failed
}

/// Sub-agent rows: what a spawned agent shows of itself.
public enum Agent {
    /// Chars a sub-agent's name keeps before it's cut.
    static let nameMax = 55
    /// Lines a brief and a report show until "Show more".
    public static let briefLines = 3
    public static let reportLines = 12
    /// Chars a line of the body holds at the chat's usual width, to guess how many lines a text wraps to.
    static let wrap = 90

    public static func mark(_ call: Call, _ agent: SubAgent) -> Mark {
        switch call.status {
        case .running: agent.transcript.turns.allSatisfy(\.blocks.isEmpty) ? .starting : .running
        case .ok: .done
        case .failed: .failed
        }
    }

    /// The task's first line, cut past 56 chars.
    public static func name(_ agent: SubAgent) -> String {
        guard let line = lines(agent.description).map({ $0.trimmingWhitespace() }).first(where: { !$0.isEmpty }) else { return "Subagent" }
        return line.count <= nameMax + 1 ? line : String(line.prefix(nameMax)) + "…"
    }

    /// What the sub-agent did: its tool calls and the notes it wrote between them.
    public static func steps(_ agent: SubAgent) -> [Block] {
        agent.transcript.turns.flatMap(\.blocks).filter {
            switch $0 {
            case .tool, .text: true
            default: false
            }
        }
    }

    /// "failed", "starting", "done" when it shows no steps, or its step count with the failed ones.
    public static func meta(_ call: Call, _ agent: SubAgent) -> String {
        let calls = steps(agent).compactMap(\.call)
        switch mark(call, agent) {
        case .failed: return "failed"
        case .starting: return "starting"
        case .done where calls.isEmpty: return "done"
        default:
            let failed = calls.filter { $0.status == .failed }.count
            let steps = "\(calls.count) step\(calls.count == 1 ? "" : "s")"
            return failed > 0 ? "\(steps), \(failed) failed" : steps
        }
    }

    /// What it reported back: the call's output, else the last thing it wrote. While it runs, only what it said so far as
    /// it waits on its own work.
    public static func report(_ call: Call, _ agent: SubAgent) -> String? {
        let output = call.output.flatMap { $0.allSatisfy(\.isWhitespace) ? nil : $0 }
        if call.status == .running { return output?.trimmingWhitespace() }
        let last = steps(agent).reversed().lazy.compactMap { if case .text(let m) = $0 { m.text } else { nil } }.first
        return (output ?? last)?.trimmingWhitespace()
    }

    /// What it was asked to do: the prompt it started on, which its task only names.
    public static func brief(_ agent: SubAgent) -> String? {
        guard let text = agent.transcript.turns.first?.prompt?.text.trimmingWhitespace(), !text.isEmpty else { return nil }
        return text
    }

    /// Its newest tool call.
    public static func latest(_ agent: SubAgent) -> Call? {
        for turn in agent.transcript.turns.reversed() {
            if let c = turn.blocks.reversed().lazy.compactMap(\.call).first { return c }
        }
        return nil
    }

    /// A call as one line of text: "Read docs/adr/0003.md", or the command it ran.
    public static func step(_ call: Call) -> String {
        let (verb, arg) = toolLine(call)
        return verb.isEmpty ? arg.string : "\(verb) \(arg.string)"
    }

    /// About how many lines `text` wraps to, without laying it out.
    public static func lineCount(_ text: String) -> Int {
        ChatModel.lines(text).map { max(($0.count + wrap - 1) / wrap, 1) }.reduce(0, +)
    }
}

/// Phases: runs of work hung off a fold's rail.
public enum Phase {
    /// Steps a live phase keeps in view; older ones scroll away above them.
    public static let liveSteps = 9

    /// The icon heading a phase: what its work mostly is.
    public static func glyph(_ blocks: ArraySlice<Block>) -> Glyph {
        switch blocks.first(where: isWork) {
        case .thinking?: return .bulb
        case .tool(let c)?:
            switch c.kind {
            case .edit, .write: return .pencil
            case .shell: return .terminal
            case .agent: return .bot
            case .read, .search, .fetch: return .search
            default: return .tool
            }
        default: return .tool
        }
    }

    /// The phase's steps: its thinking and tool calls, without the todos updated among them.
    public static func steps(_ blocks: ArraySlice<Block>) -> [(index: Int, block: Block)] {
        zip(blocks.indices, blocks).filter { isWork($0.1) }.map { ($0.0, $0.1) }
    }

    /// How long a thought took: until the next thing the agent did. Nil under a second.
    public static func thoughtFor(_ at: Int, _ next: Int?) -> String? {
        guard let next, next - at >= 1000 else { return nil }
        return "Thought for \(TimeText.elapsed(seconds: (next - at) / 1000))"
    }
}

/// When a block happened, for those that say.
public func blockAt(_ b: Block) -> Int? {
    switch b {
    case .text(let m), .thinking(let m), .plan(let m), .notice(let m, _): m.at
    case .tool(let c): c.at
    default: nil
    }
}

/// Whether `turn` is the one the agent is still working on.
func live(_ t: Transcript, _ turn: Int) -> Bool {
    turn + 1 == t.turns.count && t.turns[turn].end == nil
}

/// What a live fold says: stopping once the user stopped it, waiting on the user (`ask` true for a question), thinking
/// before anything else, else working for `now` less the start.
public func liveState(_ t: Transcript, _ turns: Range<Int>, ask: Bool?, stopping: Bool, now: Int) -> Live {
    if stopping { return .stopping }
    if let ask { return ask ? .answer : .approval }
    let background = inBackground(t.turns[...]).count
    let blocks = t.turns[turns].flatMap(\.blocks)
    if background == 0 && blocks.allSatisfy({ if case .thinking = $0 { true } else { false } }) { return .thinking }
    let secs = start(t.turns[turns.lowerBound]).map { max(now - $0, 0) / 1000 } ?? 0
    return .working(secs: secs, background: background)
}

/// How long a settled reply took, in ms: its turns' own count, else from its start to its last move.
public func took(_ t: Transcript, _ turns: Range<Int>) -> Int {
    if let (ms, _) = worked(t, turns) { return ms }
    let last = t.turns[turns].flatMap(\.blocks).compactMap(blockAt).max()
    guard let a = start(t.turns[turns.lowerBound]), let b = last else { return 0 }
    return max(b - a, 0)
}

/// Whether a phase is its turn's newest work: the turn goes on and nothing but todos came after it.
public func phaseLive(_ t: Transcript, _ turn: Int, _ blocks: Range<Int>) -> Bool {
    live(t, turn) && t.turns[turn].blocks[blocks.upperBound...].allSatisfy { if case .todos = $0 { true } else { false } }
}

/// When what follows a phase began: its next timed block, else the next turn's start.
public func nextAt(_ t: Transcript, _ turn: Int, _ end: Int) -> Int? {
    t.turns[turn].blocks[end...].lazy.compactMap(blockAt).first ?? (turn + 1 < t.turns.count ? start(t.turns[turn + 1]) : nil)
}

/// A reply's answer: the text each of its turns ends on, after its work.
public func answer(_ t: Transcript, _ turns: Range<Int>) -> String {
    t.turns[turns].flatMap { turn in
        turn.blocks[workEnd(turn)...].compactMap { b -> String? in
            switch b {
            case .text(let m), .plan(let m): m.text
            default: nil
            }
        }
    }.joined(separator: "\n\n")
}

/// Whether `row` hangs off its fold's rail: a phase, or the text between phases while the fold is open. The live tail's
/// fold shows no header, its line having moved to the bottom, so nothing hangs off it.
public func inFold(_ t: Transcript, liveTail: Int?, _ row: Row, foldOpen: Bool) -> Bool {
    guard let turn = row.turn, let fold = foldOf(t, turn), liveTail != fold else { return false }
    if case .phase = row { return true }
    return foldOpen && narration(t, row)
}

func narration(_ t: Transcript, _ row: Row) -> Bool {
    guard case .text(let turn, let block, _) = row else { return false }
    return narrates(t.turns[turn], block)
}

/// Whether a row under a fold shows: the work does while the fold is open; a phase holding a failed sub-agent always does.
public func shown(_ t: Transcript, _ row: Row, foldOpen: Bool) -> Bool {
    switch row {
    case .phase(let turn, let blocks): foldOpen || t.turns[turn].blocks[blocks].contains(where: failedAgent)
    case .tasks: foldOpen
    default: true
    }
}

func failedAgent(_ b: Block) -> Bool {
    guard case .tool(let c) = b, c.status == .failed, case .agent = c.kind else { return false }
    return true
}

/// Where the call with use id `id` sits: its turn and block.
public func findCall(_ t: Transcript, _ id: String) -> (turn: Int, block: Int)? {
    for (ix, turn) in t.turns.enumerated() {
        if let b = turn.blocks.firstIndex(where: { $0.call?.id == id }) { return (ix, b) }
    }
    return nil
}

/// "Audit deps is done", "Background command `cargo test` failed", "3 subagents are done"; backticks mark code.
public func noticeLabel(_ calls: [Call]) -> String {
    if calls.count == 1, let a = calls[0].agent { return "\(Agent.name(a)) is done" }
    if calls.count == 1, case .shell(let command) = calls[0].kind {
        let first = lines(command).first.map(String.init) ?? ""
        if calls[0].name == "Monitor" { return "Monitor `\(first)` ended" }
        return calls[0].status == .failed ? "Background command `\(first)` failed" : "Background command `\(first)` is done"
    }
    if calls.allSatisfy({ $0.agent != nil }) { return "\(calls.count) subagents are done" }
    return "\(calls.count) background tasks are done"
}

/// A label's text before, inside and after its code span; the outer backticks bound it, so a command may hold its own.
public func codeSpan(_ label: String) -> (before: String, code: String?, after: String) {
    guard let open = label.firstIndex(of: "`") else { return (label, nil, "") }
    let rest = label[label.index(after: open)...]
    guard let close = rest.lastIndex(of: "`") else { return (label, nil, "") }
    return (String(label[..<open]), String(rest[..<close]), String(rest[rest.index(after: close)...]))
}

/// The text of a text or plan block.
public func message(_ t: Transcript, _ turn: Int, _ block: Int) -> Message? {
    guard t.turns.indices.contains(turn), t.turns[turn].blocks.indices.contains(block) else { return nil }
    switch t.turns[turn].blocks[block] {
    case .text(let m), .plan(let m): return m
    default: return nil
    }
}

/// The messages of the last two turns, where new text lands.
public func latestTexts(_ t: Transcript) -> [Message] {
    t.turns.suffix(2).reversed().flatMap(\.blocks).compactMap {
        switch $0 {
        case .text(let m), .plan(let m): m
        default: nil
        }
    }
}

/// Starts or extends a reveal for each latest message longer (in UTF-8 bytes) than it was `before`, and forgets finished ones.
public func streamIn(_ reveals: inout [String: Reveal], before: [String: Int], _ t: Transcript, now: Seconds) {
    reveals = reveals.filter { $0.value.moving(now) }
    for m in latestTexts(t) {
        let was = before[m.id] ?? 0
        let len = m.text.utf8.count
        if len > was { reveals[m.id, default: .shown(was, now)].arrive(len, now) }
    }
}

/// The lengths of the latest messages, for `streamIn` to tell what grew.
public func textLengths(_ t: Transcript) -> [String: Int] {
    Dictionary(latestTexts(t).map { ($0.id, $0.text.utf8.count) }, uniquingKeysWith: { a, _ in a })
}

/// A reading width held between 560 and 1200, in 16 pt steps.
public func contentWidth(_ width: Double) -> Double {
    let width = width.isFinite ? min(max(width, 560), 1200) : 720
    return 560 + ((width - 560) / 16).rounded() * 16
}
