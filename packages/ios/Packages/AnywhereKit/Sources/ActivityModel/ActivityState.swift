import Foundation

/// A session as the activity counts it.
public struct ActivitySession: Hashable, Sendable {
    public var id: String
    /// needsYou, working, failed or done; anything else isn't counted.
    public var status: String
    /// Unix milliseconds of its last status step.
    public var updatedAt: Int

    public init(id: String, status: String, updatedAt: Int) {
        self.id = id
        self.status = status
        self.updatedAt = updatedAt
    }
}

/// An open ask as the activity shows it.
public struct ActivityAsk: Hashable, Sendable {
    public var requestId: String
    public var agentId: String
    /// "Run", "Edit"…
    public var verb: String
    public var subject: String
    /// Plain allow or deny: no options, no questions.
    public var answerable: Bool

    public init(requestId: String, agentId: String, verb: String, subject: String, answerable: Bool) {
        self.requestId = requestId
        self.agentId = agentId
        self.verb = verb
        self.subject = subject
        self.answerable = answerable
    }
}

extension MacActivity.ContentState {
    static let subjectMax = 60

    /// The state of a Mac's sessions, as pocketd's `push.Build` makes it, for the app to start or update the activity itself.
    /// The ask's verb and subject show only with `details`.
    public static func build(_ sessions: [ActivitySession], asks: [ActivityAsk], details: Bool) -> Self {
        let rank = [MacActivity.Top.needsYou: 0, MacActivity.Top.working: 1, MacActivity.Top.failed: 2, MacActivity.Top.done: 3]
        var state = Self(working: 0, needsYou: 0, unseen: 0)
        var top: ActivitySession?
        for s in sessions {
            guard let r = rank[s.status] else { continue }
            switch s.status {
            case MacActivity.Top.needsYou: state.needsYou += 1
            case MacActivity.Top.working: state.working += 1
            default: state.unseen += 1
            }
            if let t = top, let tr = rank[t.status], (tr, t.updatedAt, t.id) <= (r, s.updatedAt, s.id) { continue }
            top = s
        }
        guard let top else { return state }
        var shown = MacActivity.Top(slot: top.id, status: top.status, sinceUnix: Double(top.updatedAt) / 1000, answerable: false)
        if top.status == MacActivity.Top.needsYou, let ask = asks.first(where: { $0.agentId == top.id }) {
            if details {
                shown.verb = ask.verb
                shown.subject = cut(ask.subject, subjectMax)
            }
            if ask.answerable {
                shown.answerable = true
                shown.request = ask.requestId
            }
        }
        state.top = shown
        return state
    }

    static func cut(_ s: String, _ n: Int) -> String {
        s.count <= n ? s : String(s.prefix(n - 1)) + "…"
    }
}
