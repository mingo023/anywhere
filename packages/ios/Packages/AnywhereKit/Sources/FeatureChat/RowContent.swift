import ChatModel
import Domain
import Foundation
import Wire

/// One row as it draws: what it shows, worked out from the session alone so a row redraws only when this changes.
struct RowItem: Identifiable, Hashable, Sendable {
    let id: String
    let gap: Double
    let content: RowContent
}

enum RowContent: Hashable, Sendable {
    /// A reply's fold, which draws nothing.
    case hidden
    case prompt(PromptContent)
    case step(StepContent)
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

struct StepContent: Hashable, Sendable {
    /// The key its open state goes by.
    var id: String
    var block: Block
    var open: Bool
    /// Still thinking, or a call still running while its turn goes on.
    var live: Bool
    /// A call still running after the work it was part of settled.
    var background: Bool
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
    /// The message is still letting this block out; `RowActions.reveal` says how far, so the row doesn't change per chunk.
    var revealing: Bool
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
    /// A card in the composer's place says what the agent waits on, so the live line doesn't.
    let carded: Bool
    let canSendNow: Bool
    let now: Int

    init(
        session: Session, provider: String, model: String?, ask: Bool?, stopping: Bool, reveals: [String: Reveal] = [:],
        carded: Bool = false, canSendNow: Bool, now: Int
    ) {
        self.session = session
        self.provider = provider
        self.model = model.map(modelName) ?? ""
        self.ask = ask
        self.stopping = stopping
        self.reveals = reveals
        self.carded = carded
        self.canSendNow = canSendNow
        self.now = now
    }

    var t: Transcript { session.transcript }
    var who: String { AgentName.who(provider) }

    func item(_ ix: Int) -> RowItem {
        let row = session.rows[ix]
        let id = session.ids[ix]
        if case .fold = row { return RowItem(id: id, gap: 0, content: .hidden) }
        // A fold draws nothing, so the row above it sets the gap.
        var above = ix - 1
        if above >= 0, case .fold = session.rows[above] { above -= 1 }
        return RowItem(id: id, gap: topGap(above >= 0 ? session.rows[above] : nil, row), content: content(row, id))
    }

    func content(_ row: Row, _ id: String) -> RowContent {
        switch row {
        case .prompt(let turn):
            guard let m = t.turns[turn].prompt else { return .hidden }
            let (text, images) = splitImages(m.text)
            return .prompt(PromptContent(text: text, images: images, at: m.at))
        case .fold:
            return .hidden
        case .phase(let turn, let blocks):
            return step(turn, blocks, id)
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
            let utf8 = m.text.utf8
            let text = String(decoding: utf8.dropFirst(range.lowerBound).prefix(range.count), as: UTF8.self)
            let plan = if case .plan = t.turns[turn].blocks[block] { true } else { false }
            return .text(TextContent(message: m.id, text: text, offset: range.lowerBound, planHeader: plan && range.lowerBound == 0 && !text.hasPrefix("#"),
                                     revealing: reveals[m.id]?.reaches(before: range.upperBound) ?? false))
        case .question(let turn, let block):
            guard let call = t.turns[turn].blocks[block].call else { return .hidden }
            return .question(questionValue(call))
        case .compacted(let turn, let block):
            guard case .compacted(_, let auto) = t.turns[turn].blocks[block] else { return .hidden }
            return .compacted(auto: auto)
        case .end(let turn):
            return .end(end(turn))
        case .live(let turn):
            if carded { return .hidden }
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
            let afterTurn = session.outgoing.afterTurn.contains { $0.id == p.id }
            let (text, images) = splitImages(p.text)
            return .pending(PendingContent(
                id: p.id, text: text, images: images, delivery: p.delivery,
                waiting: last ? Bubble.waiting(who, waiting.count) : afterTurn ? Bubble.afterTurn(who) : nil,
                sendNow: last && canSendNow ? (session.outgoing.sendingNow ? .sending : .offer) : nil))
        }
    }

    func step(_ turn: Int, _ blocks: Range<Int>, _ id: String) -> RowContent {
        guard let (b, block) = Phase.steps(t.turns[turn].blocks[blocks]).first else { return .hidden }
        let live = phaseLive(t, turn, blocks)
        switch block {
        case .thinking(let m):
            let thought = live ? nil : Phase.thoughtFor(m.at, nextAt(t, turn, b + 1)) ?? "Thought"
            return .step(StepContent(id: id, block: block, open: session.open(id, false), live: live, background: false, thought: thought))
        case .tool(let c):
            let running = c.status == .running
            return .step(StepContent(
                id: id, block: block, open: session.open(id, c.agent != nil && c.status == .failed), live: running && live, background: running && !live))
        default:
            return .hidden
        }
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
