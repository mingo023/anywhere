public enum Delivery: Hashable, Sendable {
    case sending
    /// Sent into the turn underway, which takes it at its next step; it waits until its prompt reaches the transcript.
    case steered
    /// Held here while the agent works, unsent; it goes out when the turn ends, or when steered.
    case queued
    case failed

    var inFlight: Bool { self == .sending || self == .steered }
}

public struct Pending: Hashable, Sendable {
    /// Stays with the send while others ahead of it settle, so a tap finds it.
    public var id: Int
    public var text: String
    /// When it went to the agent; for a queued send, when it was queued. Its 120 s grace runs from then, or from when
    /// the agent was next seen idle, as an agent may hold a prompt sent mid-turn until the turn ends.
    public var sentAt: Int
    public var delivery: Delivery
    /// The id it went to pocketd under, which a refusal names.
    var request: String?
    /// A stop put it back in the queue after it went out; should the agent run it after all, its echo settles it.
    var stopped = false
}

/// A prompt must reach the transcript within this long of being sent, or it reads as not delivered.
let graceMs = 120_000
/// How far the agent's clock may run behind ours when its prompt item is stamped.
let skewMs = 10_000
/// Enough claimed prompts to tell a repeated send from the one before it.
let claimedMax = 32
/// How long pocketd's status may take to show a command at work; one it never shows did nothing to wait for.
let statusLagMs = 3_000
/// The longest a command reads as work, should its end never show.
let commandMs = 60_000
/// The longest a stop reads as under way, should the turn never show it ended.
let stopMs = 10_000
/// The longest a Send now reads as under way, should what waits never show.
let sendNowMs = 10_000

/// A command that went out. One that takes a turn echoes as its prompt; one that doesn't shows only in pocketd's status,
/// or the timeline moving.
struct Ran: Hashable, Sendable {
    var at: Int
    /// pocketd's status showed the agent working since.
    var worked = false
    /// pocketd's status showed the agent idle since. Until then, or `statusLagMs`, working may be the turn before's.
    var idled = false
}

/// What the user sent that the transcript doesn't show yet: on its way, steered in, queued or failed.
public struct Outgoing: Sendable {
    var sends: [Pending] = []
    /// Ids of prompts that already settled a send, so a repeat of the same text waits for its own.
    var claimed: [String] = []
    /// Queued sends `settle` let go, for the caller to send.
    var releasedSends: [(id: Int, text: String)] = []
    var nextId = 0
    /// The failed turn the user chose to carry on past.
    var resumed: String?
    /// Whether the agent was on a turn when last settled.
    var working = false
    /// When the agent was last seen to leave a turn.
    var idleSince = 0
    /// The agent refused a send, so the next would be refused too.
    var refused = false
    /// A command went out and hasn't shown its end yet.
    var command: Ran?
    /// When the user stopped the agent, until the turn shows it ended.
    var stoppingAt: Int?
    /// A send was queued while stopping: the user means to go on once the stop lands.
    var afterStop = false
    /// When the user pressed Send now, until what waited shows.
    var sentNow: Int?

    public init() {}

    /// Queued, and not to be sent yet, if the agent was working. A command sent now isn't kept: it has no echo to wait for.
    /// Returns the id of a send now on its way.
    @discardableResult
    public mutating func push(_ text: String, working: Bool, now: Int) -> Int? {
        if !working && isCommand(text) { command = Ran(at: now) }
        afterStop = afterStop || (working && stoppingAt != nil)
        nextId += 1
        sends.append(Pending(id: nextId, text: text, sentAt: now, delivery: working ? .queued : .sending))
        return working ? nil : nextId
    }

    /// Sends `text` into the turn underway; returns its id. A command shows too, until the turn it waits behind ends.
    public mutating func steerText(_ text: String, now: Int) -> Int {
        if isCommand(text) { command = Ran(at: now) }
        nextId += 1
        sends.append(Pending(id: nextId, text: text, sentAt: now, delivery: .steered))
        return nextId
    }

    /// Notes the id send `id` went to pocketd under.
    public mutating func sentAs(_ id: Int, _ request: String) {
        if let ix = sends.firstIndex(where: { $0.id == id }) { sends[ix].request = request }
    }

    /// Drops sends whose prompt reached the transcript and, while the agent is idle, fails those past their grace. Once
    /// the agent is idle with nothing on its way, and not paused, lets the oldest queued send go: one per turn, in order.
    /// `working` is pocketd's status, which decides only where the transcript can't; see `busy`.
    public mutating func settle(_ t: Transcript, working: Bool, now: Int) {
        let idle = !onTurn(t, working)
        if idle && self.working { idleSince = now }
        self.working = !idle
        if var ran = command {
            let age = now - ran.at
            ran.idled = ran.idled || !working
            ran.worked = ran.worked || (working && (ran.idled || age > statusLagMs))
            command = (ran.worked && !working) || (!ran.worked && age > statusLagMs) || age > commandMs ? nil : ran
        }
        // A command steered in waits in the agent's queue, however long the turn it came into runs, and goes as that turn
        // ends. One that takes a turn settles by its echo before then, should the agent start it before we see it idle.
        let since = idleSince
        sends.removeAll { $0.delivery == .steered && isCommand($0.text) && idle && $0.sentAt <= since }
        let before = sends.count
        var kept: [Pending] = []
        for send in sends {
            if send.delivery == .queued && !send.stopped {
                kept.append(send)
                continue
            }
            var prompt: Message?
            for turn in t.turns.reversed() {
                guard let p = turn.prompt else { continue }
                if p.at < send.sentAt - skewMs { break }
                if !claimed.contains(p.id) && sameText(p.text, send.text) { prompt = p }
            }
            if let prompt { claimed.append(prompt.id) } else { kept.append(send) }
        }
        sends = kept
        // A command that never echoes goes once it reads as done.
        if command == nil { sends.removeAll { $0.delivery == .sending && isCommand($0.text) } }
        if claimed.count > claimedMax { claimed.removeFirst(claimed.count - claimedMax) }
        if sends.count < before { refused = false }
        // The agent drops a steer it hadn't taken when stopped; it waits in the queue for the user rather than going on its own.
        if stoppingAt != nil && idle {
            for ix in sends.indices where sends[ix].delivery == .steered && !isCommand(sends[ix].text) {
                sends[ix].delivery = .queued
                sends[ix].stopped = true
                afterStop = false
            }
        }
        // A stop sent before its prompt echoed leaves pocketd idle at once, though the agent has yet to take the prompt.
        if !busy(t, self, working: working) || stoppingAt.map({ now - $0 > stopMs }) ?? false { stoppingAt = nil }
        if waiting.isEmpty || sentNow.map({ now - $0 > sendNowMs }) ?? false { sentNow = nil }
        if afterStop && stoppingAt == nil {
            afterStop = false
            resume(t)
        }
        if !idle { return }
        for ix in sends.indices where sends[ix].delivery.inFlight && !isCommand(sends[ix].text) && now - max(sends[ix].sentAt, idleSince) > graceMs {
            sends[ix].delivery = .failed
        }
        if !paused(t) && !busy(t, self, working: working), let id = sends.first(where: { $0.delivery == .queued })?.id,
            let text = release(id, now, .sending)
        {
            releasedSends.append((id, text))
        }
    }

    /// The sends steered into the turn that the agent hasn't taken yet; it takes them all at its next step. A command
    /// echoes nothing, so it never shows as waiting.
    public var waiting: [Pending] {
        sends.filter { $0.delivery == .steered && !isCommand($0.text) }
    }

    /// The commands steered into the turn; the agent runs them only once it ends.
    public var afterTurn: [Pending] {
        sends.filter { $0.delivery == .steered && isCommand($0.text) }
    }

    /// Asks for every waiting send to go now; false when none waits or a Send now is already on its way.
    public mutating func sendNow(now: Int) -> Bool {
        if waiting.isEmpty || sendingNow { return false }
        sentNow = now
        return true
    }

    /// Whether a Send now is on its way and what it was for hasn't shown yet.
    public var sendingNow: Bool { sentNow != nil }

    /// The queued sends `settle` let go since last asked, with their ids, to send to the agent.
    public mutating func released() -> [(id: Int, text: String)] {
        defer { releasedSends = [] }
        return releasedSends
    }

    /// Lets a queued send go now, even mid-turn; returns its text to send.
    public mutating func steer(_ id: Int, now: Int) -> String? {
        release(id, now, .steered)
    }

    private mutating func release(_ id: Int, _ now: Int, _ delivery: Delivery) -> String? {
        guard let ix = sends.firstIndex(where: { $0.id == id && $0.delivery == .queued }) else { return nil }
        if isCommand(sends[ix].text) { command = Ran(at: now) }
        sends[ix].delivery = delivery
        sends[ix].sentAt = now
        return sends[ix].text
    }

    /// Drops a queued send unsent, handing back its text.
    public mutating func withdraw(_ id: Int) -> String? { take(id, .queued) }

    /// Takes a failed send back for resending.
    public mutating func retry(_ id: Int) -> String? { take(id, .failed) }

    private mutating func take(_ id: Int, _ delivery: Delivery) -> String? {
        guard let ix = sends.firstIndex(where: { $0.id == id && $0.delivery == delivery }) else { return nil }
        return sends.remove(at: ix).text
    }

    /// Fails the send that went under `request` at once: the agent refused it. The queue waits until a send gets through or the user resumes.
    public mutating func fail(_ request: String) {
        guard let ix = sends.firstIndex(where: { $0.delivery.inFlight && $0.request == request }) else { return }
        sends[ix].delivery = .failed
        refused = true
    }

    /// Whether queued sends wait for the user because the last turn ended in a stop or an error, or is being stopped,
    /// or holds a steer the stop beat, or a send was refused.
    public func paused(_ t: Transcript) -> Bool {
        let last = t.turns.last
        let failed = { if case .failed? = last?.end { true } else { false } }()
        let beaten = sends.contains { $0.stopped && $0.delivery == .queued }
        let wasResumed = last.map { resumed == $0.id } ?? false
        return holds && (refused || (stoppingAt != nil || failed || beaten) && !wasResumed)
    }

    /// Lets the queue go on past the failed turn or refusal it waits behind.
    public mutating func resume(_ t: Transcript) {
        resumed = t.turns.last?.id
        refused = false
    }

    /// Takes out every send that won't reach the agent unless sent again, queued or failed, oldest first.
    public mutating func drain() -> [String] {
        let rest = sends.filter { !$0.delivery.inFlight }
        sends = sends.filter(\.delivery.inFlight)
        return rest.map(\.text)
    }

    /// Whether a send waits its turn.
    public var holds: Bool { sends.contains { $0.delivery == .queued } }

    /// The user stopped the agent: the queue pauses now rather than once the turn shows it.
    public mutating func stop(now: Int) { stoppingAt = now }

    public mutating func stopRefused() { stoppingAt = nil }

    /// Whether a stop is on its way and the turn doesn't show it yet.
    public var stopping: Bool { stoppingAt != nil }

    /// The agent's timeline moved, so a command sent before it shows what it did, if anything.
    public mutating func moved() { command = nil }

    public var list: [Pending] { sends }

    /// The sends the transcript shows, with their index in `list`; a queued one waits in the composer's tray instead.
    public var echoed: [(index: Int, pending: Pending)] {
        sends.enumerated().filter { $0.element.delivery != .queued }.map { ($0.offset, $0.element) }
    }
}

/// Whether the agent is on a turn as the chat sees it: a send steers into it and Stop shows, and the queue holds.
/// A send on its way counts, as the agent takes it as a turn. `working` is pocketd's status: the transcript shows a stop
/// at once where status trails it by up to a poll, so status decides only where the transcript can't, while it has no
/// turn (not fetched yet, or dropped as pocketd restarted). A command just sent counts until it shows its end; see `settle`.
/// Sub-agents left in the background don't: the agent takes a send as a new turn. The background tray stops them instead.
public func busy(_ t: Transcript, _ out: Outgoing, working: Bool) -> Bool {
    out.sends.contains { $0.delivery.inFlight } || out.command != nil || onTurn(t, working)
}

func onTurn(_ t: Transcript, _ working: Bool) -> Bool {
    t.turns.isEmpty ? working : atWork(t)
}

/// A slash command, which Claude may run without echoing a prompt; a prompt may start with a path instead.
func isCommand(_ text: String) -> Bool { slash(text) != nil }

/// Agents may tidy a prompt's whitespace before echoing it back, and Claude echoes a bash command as `! cmd`. Text the
/// user left in Claude's input runs on into the prompt typed after it.
func sameText(_ echo: String, _ sent: String) -> Bool {
    tidy(echo).hasSuffix(tidy(sent))
}

private func tidy(_ text: String) -> String {
    let text = text.split(whereSeparator: \.isWhitespace).joined(separator: " ")
    return text.hasPrefix("! ") ? "!" + text.dropFirst(2) : text
}

public enum Action: Hashable, Sendable {
    case send
    /// Sends into the turn underway; the agent reads it at its next step.
    case steer
    /// A stop is on its way, so the send waits for the turn to end.
    case queue
    case stop
    /// A stop is on its way; nothing to press until the turn ends.
    case stopping
}

/// Stop while working with an empty draft, Steer while working with one, else Send. Enter never stops, and once
/// `stopping`, nothing does again: a draft then queues.
public func action(working: Bool, stopping: Bool, empty: Bool) -> Action {
    switch (working, empty) {
    case (true, true): stopping ? .stopping : .stop
    case (true, false): stopping ? .queue : .steer
    case (false, _): .send
    }
}
