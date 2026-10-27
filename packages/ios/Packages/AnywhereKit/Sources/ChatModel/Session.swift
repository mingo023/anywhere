import Wire

/// How the rows moved: those in `remeasure` hold their ids but show something new, and the `replace` range's rows
/// gave way to `count` others.
public struct Change: Hashable, Sendable {
    public var remeasure: Range<Int>
    public var replace: Range<Int>
    public var count: Int

    public init(_ remeasure: Range<Int>, _ replace: Range<Int>, _ count: Int) {
        self.remeasure = remeasure
        self.replace = replace
        self.count = count
    }
}

/// One agent's chat as the view shows it; rebuilt only when the agent's items change.
public struct Session: Sendable {
    public private(set) var transcript = Transcript()
    public private(set) var rows: [Row] = []
    public private(set) var ids: [String] = []
    /// Open/closed choices the user made, by row id: folds, phases and sub-agents.
    public var folds: [String: Bool] = [:]
    public var outgoing = Outgoing()
    var revision = 0
    var fold = Fold()
    /// Sends the transcript shows, and whether one is still on its way.
    var sends = (count: 0, sending: false)

    public init() {}

    /// Refolds when `revision` moved, settles outgoing sends by the transcript and pocketd's status (`working`), and
    /// returns how the rows moved, if they did. `unchanged` items at the front are the same as at the last revision.
    @discardableResult
    public mutating func update(_ revision: Int, _ items: [TimelineItem], working: Bool, now: Int, unchanged: Int = 0) -> Change? {
        var old: (Int, [Turn])?
        if revision != self.revision {
            self.revision = revision
            outgoing.moved()
            let turns = transcript.turns.count
            let replaced = fold.refold(items, &transcript, unchanged: unchanged)
            old = (turns - replaced.count, replaced)
        }
        outgoing.settle(transcript, working: working, now: now)
        let echoed = outgoing.echoed
        let sends = (echoed.count, echoed.contains { $0.pending.delivery == .sending })
        if old == nil && sends == self.sends { return nil }
        self.sends = sends
        let (from, replaced) = old ?? (transcript.turns.count, [])
        return resplice(from, replaced)
    }

    /// Shows a transcript folded elsewhere, such as a sub-agent's inside its parent's, and returns how the rows moved, if they did.
    @discardableResult
    public mutating func show(_ transcript: Transcript) -> Change? {
        if transcript == self.transcript { return nil }
        fold = Fold()
        let old = self.transcript
        self.transcript = transcript
        return resplice(0, old.turns)
    }

    /// Rebuilds the rows of turns from `from` on, which `old` held before, and the trailer under them.
    private mutating func resplice(_ from: Int, _ old: [Turn]) -> Change? {
        var lo = 0
        var hi = rows.count
        while lo < hi {
            let mid = (lo + hi) / 2
            if let turn = rows[mid].turn, turn < from { lo = mid + 1 } else { hi = mid }
        }
        let keep = lo
        let oldRows = Array(rows[keep...])
        rows.removeSubrange(keep...)
        rowsFrom(&rows, transcript, from, pending: sends.count, sending: sends.sending)
        let newIds = rows[keep...].map { $0.id(transcript) }
        let (range, count) = splice(Array(ids[keep...]), newIds)
        // Ids hold while a row's content changes (a tool finishing, text streaming in): the list measures those rows
        // afresh where they are, keeping their height until then, rather than replacing them.
        let first = (0..<range.lowerBound).first { oldRows[$0] != rows[keep + $0] || !sameContent(old, from, transcript, rows[keep + $0]) } ?? range.lowerBound
        ids.removeSubrange(keep...)
        ids.append(contentsOf: newIds)
        guard first < range.upperBound || count > 0 else { return nil }
        return Change(keep + first..<keep + range.lowerBound, keep + range.lowerBound..<keep + range.upperBound, count)
    }

    /// Whether the row `id` shows open: the user's choice, else `default`.
    public func open(_ id: String, _ fallback: Bool) -> Bool {
        folds[id] ?? fallback
    }

    public mutating func toggle(_ id: String, _ fallback: Bool) {
        folds[id] = !open(id, fallback)
    }

    /// The fold over row `ix` when it shows shut, to open so the row shows; none while work going on holds it open.
    public func shutFoldOver(_ ix: Int) -> String? {
        let t = transcript
        guard let turn = rows[ix].turn, let f = foldOf(t, turn), !atWorkOn(t, folded(t, f)) else { return nil }
        let id = "\(t.turns[f].id)#fold"
        return open(id, false) ? nil : id
    }

    /// The fold whose live line shows under the newest exchange, read off the trailer rather than the whole transcript.
    public var liveTail: Int? {
        if case .live(let turn)? = rows.last { turn } else { nil }
    }

    /// Whether row `ix` is a phase that shows shut.
    public func shutPhase(_ ix: Int) -> Bool {
        guard case .phase = rows[ix] else { return false }
        return !open(ids[ix], false)
    }
}

/// Whether `row`, of a turn from `from` on, shows what it showed when `old` held those turns.
func sameContent(_ old: [Turn], _ from: Int, _ new: Transcript, _ row: Row) -> Bool {
    guard let turn = row.turn else { return true }
    guard turn - from >= 0, turn - from < old.count, turn < new.turns.count else { return false }
    let a = old[turn - from]
    let b = new.turns[turn]
    switch row {
    case .prompt: return a.prompt == b.prompt
    case .text(_, let block, let range): return slice(text(a, block), range) == slice(text(b, block), range)
    case .phase(_, let blocks): return slice(a.blocks, blocks) == slice(b.blocks, blocks)
    case .tasks(_, let block), .question(_, let block), .shared(_, let block), .compacted(_, let block), .note(_, let block):
        return slice(a.blocks, block..<block + 1) == slice(b.blocks, block..<block + 1)
    // One line, whatever the reply it heads does.
    case .fold, .notice: return true
    case .end: return a.end == b.end
    case .live, .working, .pending: return true
    }
}

private func slice<T: Equatable>(_ a: [T], _ r: Range<Int>) -> ArraySlice<T>? {
    r.upperBound <= a.count ? a[r] : nil
}

private func slice(_ s: String, _ r: Range<Int>) -> [UInt8]? {
    let u = s.utf8
    guard r.upperBound <= u.count else { return nil }
    return Array(u[u.index(u.startIndex, offsetBy: r.lowerBound)..<u.index(u.startIndex, offsetBy: r.upperBound)])
}

func text(_ turn: Turn, _ block: Int) -> String {
    guard turn.blocks.indices.contains(block) else { return "" }
    switch turn.blocks[block] {
    case .text(let m), .plan(let m): return m.text
    default: return ""
    }
}
