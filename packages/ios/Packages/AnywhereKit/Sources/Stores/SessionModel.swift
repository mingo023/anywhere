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
    /// Open asks in the order they came.
    public internal(set) var requests: [PermissionRequest] = []
    /// No attempt named the session from its prompt.
    public internal(set) var namingFailed = false

    init(_ summary: AgentSummary) {
        id = summary.id
        self.summary = summary
        status = Self.status(summary)
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
        return ranked != before
    }

    static func status(_ s: AgentSummary) -> Status {
        Status(status: s.status.rawValue, failed: s.failed ?? false, attached: s.attached)
    }
}
