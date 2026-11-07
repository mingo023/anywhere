/// One task of a team's plan, done by one worker (desktop only).
public struct TeamTask: Codable, Hashable, Sendable, Identifiable {
    public var id: String
    public var title: String
    public var agent: ProviderId
    public var model: String?
    public var files: [String]
    public var after: [String]
    public var allow: [String]
    public var instructions: String
    public var state: String
    public var worker: String?
    public var branch: String?
    public var worktree: String?
    public var summary: String?
    public var stat: String?
    public var turn: String?
}

/// A lead's plan and how its tasks stand (desktop only).
public struct Team: Codable, Hashable, Sendable {
    public var lead: String
    public var state: String
    public var error: String?
    public var tasks: [TeamTask]
    public var ready: Bool?
    public var conflict: [String]?
    public var repaired: Bool?
    public var checkout: String?
    /// Unix milliseconds.
    public var updatedAt: Int
}

/// Every lead's team, the reply to team.list.
public struct TeamList: Codable, Hashable, Sendable {
    public var teams: [Team]
}

/// A team after any change.
public struct TeamUpdate: Codable, Hashable, Sendable {
    public var team: Team
}

extension ClientMessage {
    /// A task kept at team.start (desktop only).
    public struct TaskPick: Codable, Hashable, Sendable {
        public var id: String
        public var agent: ProviderId
        public var model: String?
        public var allow: [String]?
    }

    /// team.start: the lead and the tasks kept.
    public struct TeamStart: Codable, Hashable, Sendable {
        public var lead: String
        public var tasks: [TaskPick]
    }

    /// team.stop: one task of the lead's team.
    public struct TeamStop: Codable, Hashable, Sendable {
        public var lead: String
        public var task: String
    }

    /// team.merge: the lead whose team merges.
    public struct TeamRef: Codable, Hashable, Sendable {
        public var lead: String
    }
}
