import Domain
import Observation
import Wire

/// One agent's card. Its own object, so a change to one agent redraws one card.
@MainActor
@Observable
public final class SessionModel: Identifiable {
    public let id: String
    public private(set) var summary: AgentSummary
    public private(set) var status: Status
    /// What the home list places and sorts it by; activity alone leaves it be, so the list isn't rebuilt for it.
    public private(set) var listing: HomeList.Session
    /// Open asks in the order they came.
    public internal(set) var requests: [PermissionRequest] = []
    /// No attempt named the session from its prompt.
    public internal(set) var namingFailed = false

    init(_ summary: AgentSummary) {
        id = summary.id
        self.summary = summary
        let status = Self.status(summary)
        self.status = status
        listing = Self.listing(summary, status)
    }

    var ranked: Ranked {
        Ranked(id: id, status: status, updatedAt: summary.updatedAt, createdAt: summary.createdAt)
    }

    /// Takes a newer summary; true when its place in the lists may have moved.
    func update(_ new: AgentSummary) -> Bool {
        guard new != summary else { return false }
        let before = ranked
        summary = new
        let status = Self.status(new)
        if status != self.status { self.status = status }
        let listing = Self.listing(new, status)
        if listing != self.listing { self.listing = listing }
        return ranked != before
    }

    static func listing(_ s: AgentSummary, _ status: Status) -> HomeList.Session {
        HomeList.Session(
            card: .init(id: s.id, title: s.title, status: status, createdAt: s.createdAt, updatedAt: s.updatedAt, pinned: s.pinned ?? false),
            cwd: s.cwd, terminalId: s.terminalId)
    }

    static func status(_ s: AgentSummary) -> Status {
        Status(status: s.status.rawValue, failed: s.failed ?? false, attached: s.attached)
    }
}
