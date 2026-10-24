import ChatModel
import Domain
import Foundation
import Wire

/// Where a row sits on its fold's rail: hung off it, or the last there, whose spine stops at its elbow.
enum Rail: Hashable, Sendable {
    case none, mid, last
}

/// One row as it draws: what it shows, worked out from the session alone so a row redraws only when this changes.
struct RowItem: Identifiable, Hashable, Sendable {
    let id: String
    let gap: Double
    let rail: Rail
    let content: RowContent
}

enum RowContent: Hashable, Sendable {
    /// Under a shut fold, or the live reply's fold, whose line moved to the bottom.
    case hidden
    case prompt(PromptContent)
    case fold(label: String, open: Bool, settled: Bool)
    case phase(PhaseContent)
    /// What finished in the background; `call` is the first, whose phase a tap shows.
    case notice(label: String, call: String?)
    case note(text: String, error: Bool)
    case tasks([Todo])
    case text(TextContent)
    case question(String)
    case compacted(auto: Bool)
    case end(EndContent)
    case live(LiveContent)
    case working
    case pending(PendingContent)
}

struct PromptContent: Hashable, Sendable {
    var text: String
    var images: [String]
    var at: Int
}

struct PhaseContent: Hashable, Sendable {
    var id: String
    var title: String
    var glyph: Glyph
    var live: Bool
    var open: Bool
    /// A phase of one step that isn't a sub-agent shows just that step.
    var lone: Bool
    var steps: [StepContent]
}

struct StepContent: Hashable, Sendable, Identifiable {
    /// The key its open state goes by.
    var id: String
    var block: Block
    var open: Bool
    var live: Bool
    /// A thought's header once it's done: "Thought for 3s", or "Thought".
    var thought: String?
}

struct TextContent: Hashable, Sendable {
    var message: String
    var text: String
    /// Where `text` starts in the message, in UTF-8 bytes.
    var offset: Int
    /// The first block of a plan that doesn't open on its own heading.
    var planHeader: Bool
    var reveal: Reveal?
}

enum EndContent: Hashable, Sendable {
    case interrupted(String)
    case failed(String)
    case done(Footer?)
}

struct Footer: Hashable, Sendable {
    var answer: String
    /// The turn whose prompt a retry sends again.
    var retry: Int?
    var stats: String
}

struct LiveContent: Hashable, Sendable {
    var who: String
    var model: String
    var live: Live
    /// When the reply began, Unix ms, so the elapsed time ticks without the row being rebuilt.
    var start: Int?

    func label(now: Int) -> String {
        if case .working(_, let background) = live, let start {
            return liveLabel(who, model, .working(secs: max(now - start, 0) / 1000, background: background))
        }
        return liveLabel(who, model, live)
    }
}

struct PendingContent: Hashable, Sendable {
    enum SendNow: Hashable, Sendable {
        case offer, sending
    }

    var id: Int
    var text: String
    var images: [String]
    var delivery: Delivery
    /// Under the last steer the agent hasn't taken: when it will.
    var waiting: String?
    var sendNow: SendNow?
}

/// Builds the rows the list shows, one at a time, as the list asks for them.
struct RowBuilder {
    let session: Session
    let provider: String
    let model: String
    let ask: Bool?
    let stopping: Bool
    let reveals: [String: Reveal]
    let settled: String?
    let canSendNow: Bool
    let now: Int
    let liveTail: Int?

    init(
        session: Session, provider: String, model: String?, ask: Bool?, stopping: Bool, reveals: [String: Reveal] = [:],
        settled: String? = nil, canSendNow: Bool, now: Int
    ) {
        self.session = session
        self.provider = provider
        self.model = model.map(modelName) ?? ""
        self.ask = ask
        self.stopping = stopping
        self.reveals = reveals
        self.settled = settled
        self.canSendNow = canSendNow
        self.now = now
        liveTail = session.liveTail
    }

    var t: Transcript { session.transcript }
    var who: String { ChatModel.who(provider) }

    /// Whether the fold over `turn` shows open: the user's choice, or held open while its reply is at work.
    func foldOpen(_ turn: Int) -> Bool {
        guard let f = foldOf(t, turn) else { return false }
        return session.open("\(t.turns[f].id)#fold", false) || atWorkOn(t, folded(t, f))
    }

    func visible(_ row: Row) -> Bool {
        if case .fold(let turn) = row, turn == liveTail { return false }
        return shown(t, row, foldOpen: row.turn.map(foldOpen) ?? false)
    }

    func railed(_ ix: Int) -> Bool {
        let row = session.rows[ix]
        return visible(row) && inFold(t, liveTail: liveTail, row, foldOpen: row.turn.map(foldOpen) ?? false)
    }

    func item(_ ix: Int) -> RowItem {
        let row = session.rows[ix]
        let id = session.ids[ix]
        guard visible(row) else { return RowItem(id: id, gap: 0, rail: .none, content: .hidden) }
        let rail: Rail = railed(ix) ? (ix + 1 < session.rows.count && railed(ix + 1) ? .mid : .last) : .none
        let prev = ix > 0 ? session.rows[ix - 1] : nil
        return RowItem(id: id, gap: topGap(prev, row), rail: rail, content: content(row, id))
    }

    func content(_ row: Row, _ id: String) -> RowContent {
        switch row {
        case .prompt(let turn):
            guard let m = t.turns[turn].prompt else { return .hidden }
            let (text, images) = splitImages(m.text)
            return .prompt(PromptContent(text: text, images: images, at: m.at))
        case .fold(let turn):
            let reply = folded(t, turn)
            return .fold(label: settledLabel(who, took(t, reply)), open: foldOpen(turn), settled: settled == id)
        case .phase(let turn, let blocks):
            return .phase(phase(turn, blocks, id))
        case .notice(let turn):
            let calls = reported(t, turn)
            return .notice(label: noticeLabel(calls), call: calls.first?.id)
        case .note(let turn, let block):
            guard case .notice(let m, let error) = t.turns[turn].blocks[block] else { return .hidden }
            let first = m.text.split(separator: "\n").first.map(String.init) ?? m.text
            return .note(text: first, error: error)
        case .tasks(let turn, let block):
            guard case .todos(_, let todos) = t.turns[turn].blocks[block] else { return .hidden }
            return .tasks(todos)
        case .text(let turn, let block, let range):
            guard let m = message(t, turn, block) else { return .hidden }
            let utf8 = Array(m.text.utf8)
            let text = String(decoding: utf8[min(range.lowerBound, utf8.count)..<min(range.upperBound, utf8.count)], as: UTF8.self)
            let plan = if case .plan = t.turns[turn].blocks[block] { true } else { false }
            return .text(TextContent(message: m.id, text: text, offset: range.lowerBound, planHeader: plan && range.lowerBound == 0 && !text.hasPrefix("#"),
                                     reveal: reveals[m.id]))
        case .question(let turn, let block):
            guard let call = t.turns[turn].blocks[block].call else { return .hidden }
            return .question(questionValue(call))
        case .compacted(let turn, let block):
            guard case .compacted(_, let auto) = t.turns[turn].blocks[block] else { return .hidden }
            return .compacted(auto: auto)
        case .end(let turn):
            return .end(end(turn))
        case .live(let turn):
            let reply = folded(t, turn)
            let first = t.turns[reply.lowerBound]
            let start = first.prompt?.at ?? first.blocks.lazy.compactMap(blockAt).first
            return .live(LiveContent(who: who, model: model, live: liveState(t, reply, ask: ask, stopping: stopping, now: now), start: start))
        case .working:
            return .working
        case .pending(let i):
            let echoed = session.outgoing.echoed
            guard echoed.indices.contains(i) else { return .hidden }
            let p = echoed[i].pending
            let waiting = session.outgoing.waiting
            let last = waiting.last?.id == p.id
            let (text, images) = splitImages(p.text)
            return .pending(PendingContent(
                id: p.id, text: text, images: images, delivery: p.delivery, waiting: last ? Bubble.waiting(who, waiting.count) : nil,
                sendNow: last && canSendNow ? (session.outgoing.sendingNow ? .sending : .offer) : nil))
        }
    }

    func phase(_ turn: Int, _ blocks: Range<Int>, _ id: String) -> PhaseContent {
        let slice = t.turns[turn].blocks[blocks]
        let all = Phase.steps(slice)
        let live = phaseLive(t, turn, blocks)
        let open = live || session.open(id, all.contains { runningAgent($0.block) })
        let lastIndex = all.last?.index
        let steps = (live ? Array(all.suffix(Phase.liveSteps)) : all).map { (b, block) -> StepContent in
            let key = "\(id)/\(b)"
            switch block {
            case .thinking(let m):
                let stepLive = live && b == lastIndex
                let thought = stepLive ? nil : Phase.thoughtFor(m.at, nextAt(t, turn, b + 1)) ?? "Thought"
                return StepContent(id: key, block: block, open: session.open(key, false), live: stepLive, thought: thought)
            case .tool(let c):
                return StepContent(id: key, block: block, open: session.open(key, c.agent != nil && c.status == .failed), live: c.status == .running)
            default:
                return StepContent(id: key, block: block, open: false, live: false)
            }
        }
        return PhaseContent(
            id: id, title: phaseTitle(slice, live: live), glyph: Phase.glyph(slice), live: live, open: open,
            lone: all.count == 1 && all[0].block.call?.agent == nil, steps: steps)
    }

    func end(_ turn: Int) -> EndContent {
        switch t.turns[turn].end {
        case .failed(let error, _)?:
            return interrupted(error) ? .interrupted(interruptedNotice(who)) : .failed(error)
        default:
            guard let reply = reply(t, turn), let f = foldOf(t, turn), !atWorkOn(t, folded(t, f)) else { return .done(nil) }
            let first = t.turns[reply.lowerBound]
            let ms = took(t, reply)
            var stats = [TimeText.elapsed(seconds: ms / 1000)]
            if let at = first.prompt?.at ?? first.blocks.lazy.compactMap(blockAt).first { stats.append(TimeText.clock(ms: at + ms)) }
            return .done(Footer(answer: answer(t, reply), retry: first.prompt != nil ? reply.lowerBound : nil, stats: stats.joined(separator: " · ")))
        }
    }
}
