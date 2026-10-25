/// When an automation fires: chosen weekdays at a time, or every few minutes.
public enum Schedule: Codable, Hashable, Sendable {
    /// `days` 0 = Sunday … 6 = Saturday; `time` "HH:MM".
    case days([Int], time: String)
    case interval(everyMin: Int)

    enum Keys: String, CodingKey {
        case kind, days, time, everyMin
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: Keys.self)
        switch try c.decode(String.self, forKey: .kind) {
        case "days": self = .days(try c.decode([Int].self, forKey: .days), time: try c.decode(String.self, forKey: .time))
        case "interval": self = .interval(everyMin: try c.decode(Int.self, forKey: .everyMin))
        case let kind: throw malformed(decoder, "unknown schedule kind \(kind)")
        }
    }

    public func encode(to encoder: any Encoder) throws {
        var c = encoder.container(keyedBy: Keys.self)
        switch self {
        case .days(let days, let time):
            try c.encode("days", forKey: .kind)
            try c.encode(days, forKey: .days)
            try c.encode(time, forKey: .time)
        case .interval(let every):
            try c.encode("interval", forKey: .kind)
            try c.encode(every, forKey: .everyMin)
        }
    }
}

public struct Automation: Codable, Hashable, Sendable, Identifiable {
    public var id: String
    public var name: String
    public var prompt: String
    public var provider: ProviderId
    public var folder: String
    public var schedule: Schedule
    public var enabled: Bool
    public var access: Access?
    public var newWorktree: Bool?
    /// Unix milliseconds.
    public var nextRunAt: Int?

    public init(
        id: String, name: String, prompt: String, provider: ProviderId, folder: String, schedule: Schedule, enabled: Bool,
        access: Access? = nil, newWorktree: Bool? = nil, nextRunAt: Int? = nil
    ) {
        self.id = id
        self.name = name
        self.prompt = prompt
        self.provider = provider
        self.folder = folder
        self.schedule = schedule
        self.enabled = enabled
        self.access = access
        self.newWorktree = newWorktree
        self.nextRunAt = nextRunAt
    }
}

/// An automation to save; no `id` makes a new one.
public struct AutomationDraft: Codable, Hashable, Sendable {
    public var id: String?
    public var name: String
    public var prompt: String
    public var provider: ProviderId
    public var folder: String
    public var schedule: Schedule
    public var enabled: Bool
    public var access: Access?
    public var newWorktree: Bool?

    public init(
        id: String? = nil, name: String, prompt: String, provider: ProviderId, folder: String, schedule: Schedule, enabled: Bool,
        access: Access? = nil, newWorktree: Bool? = nil
    ) {
        self.id = id
        self.name = name
        self.prompt = prompt
        self.provider = provider
        self.folder = folder
        self.schedule = schedule
        self.enabled = enabled
        self.access = access
        self.newWorktree = newWorktree
    }
}

public struct RunStatus: OpenString {
    public let rawValue: String
    public init(rawValue: String) { self.rawValue = rawValue }
    public static let pending: Self = "pending"
    public static let running: Self = "running"
    public static let waiting: Self = "waiting"
    public static let succeeded: Self = "succeeded"
    public static let failed: Self = "failed"
    public static let skipped: Self = "skipped"
    public static let cancelled: Self = "cancelled"
}

/// One firing of an automation.
public struct Run: Codable, Hashable, Sendable, Identifiable {
    public var id: String
    public var automationId: String
    public var status: RunStatus
    /// "schedule" or "manual".
    public var trigger: String
    public var why: String?
    public var summary: String?
    /// Unix milliseconds.
    public var startedAt: Int
    public var finishedAt: Int?
    public var agentId: String?
    public var terminalId: String?
    public var access: String?
    public var worktree: String?

    public init(
        id: String, automationId: String, status: RunStatus, trigger: String, why: String? = nil, summary: String? = nil,
        startedAt: Int, finishedAt: Int? = nil, agentId: String? = nil, terminalId: String? = nil, access: String? = nil,
        worktree: String? = nil
    ) {
        self.id = id
        self.automationId = automationId
        self.status = status
        self.trigger = trigger
        self.why = why
        self.summary = summary
        self.startedAt = startedAt
        self.finishedAt = finishedAt
        self.agentId = agentId
        self.terminalId = terminalId
        self.access = access
        self.worktree = worktree
    }
}

/// Every automation and its recent runs, sent whole on hello and on each change (owner only).
public struct Automations: Codable, Hashable, Sendable {
    public var automations: [Automation]
    public var runs: [Run]
    public init(automations: [Automation], runs: [Run]) {
        self.automations = automations
        self.runs = runs
    }
}

extension ClientMessage {
    public struct AutomationSave: Codable, Hashable, Sendable {
        public var automation: AutomationDraft
        public init(automation: AutomationDraft) { self.automation = automation }
    }

    public struct AutomationEnable: Codable, Hashable, Sendable {
        public var automationId: String
        public var enabled: Bool
        public init(automationId: String, enabled: Bool) {
            self.automationId = automationId
            self.enabled = enabled
        }
    }

    public struct AutomationRef: Codable, Hashable, Sendable {
        public var automationId: String
        public init(automationId: String) { self.automationId = automationId }
    }
}
