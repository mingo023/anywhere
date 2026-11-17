/// A child session's delegation, on its `AgentSummary`.
public struct DelegationRef: Codable, Hashable, Sendable {
    public var taskId: String
    public var status: Delegation.Status
    public var workState: String?

    public init(taskId: String, status: Delegation.Status, workState: String? = nil) {
        self.taskId = taskId
        self.status = status
        self.workState = workState
    }
}

/// One session a chat delegated to, as its panel shows it.
public struct Delegation: Codable, Hashable, Sendable, Identifiable {
    public struct Status: OpenString {
        public let rawValue: String
        public init(rawValue: String) { self.rawValue = rawValue }
        public static let queued: Self = "queued"
        public static let starting: Self = "starting"
        public static let working: Self = "working"
        public static let completed: Self = "completed"
        public static let failed: Self = "failed"
        public static let cancelled: Self = "cancelled"
    }

    public var taskId: String
    public var parentId: String
    public var childId: String?
    public var title: String
    public var agent: String
    public var model: String?
    public var status: Status
    public var summary: String?
    public var error: String?
    public var question: String?
    /// Unix milliseconds.
    public var createdAt: Int
    /// Unix milliseconds.
    public var settledAt: Int?
    /// The host a task runs on; `childId` then carries that host's tag, `<hostId>/<theirId>`.
    public var environmentId: String?

    public var id: String { taskId }

    public init(
        taskId: String, parentId: String, childId: String? = nil, title: String, agent: String, model: String? = nil, status: Status,
        summary: String? = nil, error: String? = nil, question: String? = nil, createdAt: Int, settledAt: Int? = nil, environmentId: String? = nil
    ) {
        self.taskId = taskId
        self.parentId = parentId
        self.childId = childId
        self.title = title
        self.agent = agent
        self.model = model
        self.status = status
        self.summary = summary
        self.error = error
        self.question = question
        self.createdAt = createdAt
        self.settledAt = settledAt
        self.environmentId = environmentId
    }
}

/// `agent.delegations`: every delegation of one chat.
public struct AgentDelegations: Codable, Hashable, Sendable {
    public var agentId: String
    public var items: [Delegation]
    public init(agentId: String, items: [Delegation]) {
        self.agentId = agentId
        self.items = items
    }
}

public struct DelegationUpdate: Codable, Hashable, Sendable {
    public var agentId: String
    public var delegation: Delegation
    public init(agentId: String, delegation: Delegation) {
        self.agentId = agentId
        self.delegation = delegation
    }
}

/// A timeline item: what pocketd told the parent about its children. The arrays run in step.
public struct ChildDelivery: Codable, Hashable, Sendable {
    public struct Outcome: OpenString {
        public let rawValue: String
        public init(rawValue: String) { self.rawValue = rawValue }
        public static let done: Self = "done"
        public static let failed: Self = "failed"
        public static let asks: Self = "asks"
    }

    public var taskIds: [String]
    public var childIds: [String]
    public var titles: [String]
    public var agents: [String]
    public var outcome: Outcome
    public var question: String?

    public init(taskIds: [String], childIds: [String], titles: [String], agents: [String], outcome: Outcome, question: String? = nil) {
        self.taskIds = taskIds
        self.childIds = childIds
        self.titles = titles
        self.agents = agents
        self.outcome = outcome
        self.question = question
    }
}

/// A message waiting for its session's turn to end.
public struct QueueEntry: Codable, Hashable, Sendable, Identifiable {
    public struct Origin: OpenString {
        public let rawValue: String
        public init(rawValue: String) { self.rawValue = rawValue }
        public static let user: Self = "user"
        public static let wake: Self = "wake"
        public static let question: Self = "question"
        public static let context: Self = "context"
    }

    public struct State: OpenString {
        public let rawValue: String
        public init(rawValue: String) { self.rawValue = rawValue }
        public static let pending: Self = "pending"
        public static let sending: Self = "sending"
        public static let held: Self = "held"
    }

    public var id: String
    public var text: String
    public var origin: Origin
    public var state: State
    public var order: Double

    public init(id: String, text: String, origin: Origin, state: State, order: Double) {
        self.id = id
        self.text = text
        self.origin = origin
        self.state = state
        self.order = order
    }
}

/// `queue.list` and `queue.update`: a chat's whole queue.
public struct QueueList: Codable, Hashable, Sendable {
    public var agentId: String
    public var entries: [QueueEntry]
    public init(agentId: String, entries: [QueueEntry]) {
        self.agentId = agentId
        self.entries = entries
    }
}

extension ClientMessage {
    /// `queue.cancel` and `queue.send_now`.
    public struct QueueEntryRef: Codable, Hashable, Sendable {
        public var agentId: String
        public var entryId: String
        public init(agentId: String, entryId: String) {
            self.agentId = agentId
            self.entryId = entryId
        }
    }

    public struct QueueEdit: Codable, Hashable, Sendable {
        public var agentId: String
        public var entryId: String
        public var text: String
        public init(agentId: String, entryId: String, text: String) {
            self.agentId = agentId
            self.entryId = entryId
            self.text = text
        }
    }

    /// Moves the entry before `before`, or to the end without it.
    public struct QueueReorder: Codable, Hashable, Sendable {
        public var agentId: String
        public var entryId: String
        public var before: String?
        public init(agentId: String, entryId: String, before: String? = nil) {
            self.agentId = agentId
            self.entryId = entryId
            self.before = before
        }
    }
}
