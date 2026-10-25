public struct AgentStatus: OpenString {
    public let rawValue: String
    public init(rawValue: String) { self.rawValue = rawValue }
    public static let needsYou: Self = "needsYou"
    public static let done: Self = "done"
    public static let working: Self = "working"
    public static let idle: Self = "idle"
    public static let closed: Self = "closed"
}

/// How an agent came back after a pocketd restart, until its next turn.
public struct RestoreOutcome: OpenString {
    public let rawValue: String
    public init(rawValue: String) { self.rawValue = rawValue }
    public static let resumed: Self = "resumed"
    public static let interrupted: Self = "interrupted"
    public static let accessLowered: Self = "access_lowered"
    public static let failed: Self = "failed"
}

/// An agent's `/goal`. `status` is Codex's (active, paused, blocked, usageLimited, budgetLimited, complete) or "failed".
public struct Goal: Codable, Hashable, Sendable {
    public var objective: String
    public var status: String
    public var tokensUsed: Int?
    public var tokenBudget: Int?

    public init(objective: String, status: String, tokensUsed: Int? = nil, tokenBudget: Int? = nil) {
        self.objective = objective
        self.status = status
        self.tokensUsed = tokensUsed
        self.tokenBudget = tokenBudget
    }
}

public struct AgentSummary: Codable, Hashable, Sendable, Identifiable {
    public var id: String
    public var terminalId: String
    public var title: String
    public var cwd: String
    public var provider: String
    public var model: String?
    public var effort: String?
    public var access: String?
    public var status: AgentStatus
    public var failed: Bool?
    public var attached: Bool
    public var restore: RestoreOutcome?
    public var compacting: Bool?
    public var pinned: Bool?
    public var epoch: Int
    public var maxSeq: Int
    /// Timeline clears; when it moves, held items are stale.
    public var resets: Int?
    public var providerSessionId: String?
    /// Unix milliseconds.
    public var createdAt: Int
    /// Unix milliseconds; only a status step moves it.
    public var updatedAt: Int
    public var project: String?
    public var worktree: String?
    public var mainWorktree: Bool?
    public var branch: String?
    public var tokensUsed: Int?
    public var contextWindow: Int?
    public var origin: String?
    public var activity: String?
    /// The managed account the agent runs on; none is the built-in login.
    public var account: String?
    /// The account the agent moves to once its turn ends, "builtin" for the built-in login.
    public var switching: String?
    /// Run without its TUI: only the chat drives it.
    public var chatOnly: Bool?
    public var goal: Goal?

    public init(
        id: String, terminalId: String, title: String, cwd: String, provider: String, model: String? = nil, effort: String? = nil,
        access: String? = nil, status: AgentStatus, failed: Bool? = nil, attached: Bool, restore: RestoreOutcome? = nil,
        compacting: Bool? = nil, pinned: Bool? = nil, epoch: Int, maxSeq: Int, resets: Int? = nil, providerSessionId: String? = nil,
        createdAt: Int, updatedAt: Int, project: String? = nil, worktree: String? = nil, mainWorktree: Bool? = nil, branch: String? = nil,
        tokensUsed: Int? = nil, contextWindow: Int? = nil, origin: String? = nil, activity: String? = nil, account: String? = nil,
        switching: String? = nil, chatOnly: Bool? = nil, goal: Goal? = nil
    ) {
        self.id = id
        self.terminalId = terminalId
        self.title = title
        self.cwd = cwd
        self.provider = provider
        self.model = model
        self.effort = effort
        self.access = access
        self.status = status
        self.failed = failed
        self.attached = attached
        self.restore = restore
        self.compacting = compacting
        self.pinned = pinned
        self.epoch = epoch
        self.maxSeq = maxSeq
        self.resets = resets
        self.providerSessionId = providerSessionId
        self.createdAt = createdAt
        self.updatedAt = updatedAt
        self.project = project
        self.worktree = worktree
        self.mainWorktree = mainWorktree
        self.branch = branch
        self.tokensUsed = tokensUsed
        self.contextWindow = contextWindow
        self.origin = origin
        self.activity = activity
        self.account = account
        self.switching = switching
        self.chatOnly = chatOnly
        self.goal = goal
    }
}

/// `agent.list`: every agent, as the reply to a list or pushed whole.
public struct AgentList: Codable, Hashable, Sendable {
    public var agents: [AgentSummary]
    public init(agents: [AgentSummary]) { self.agents = agents }
}

public struct AgentUpdate: Codable, Hashable, Sendable {
    public var agent: AgentSummary
    public init(agent: AgentSummary) { self.agent = agent }
}

public struct Command: Codable, Hashable, Sendable {
    public var name: String
    public var description: String?
    public var argumentHint: String?

    public init(name: String, description: String? = nil, argumentHint: String? = nil) {
        self.name = name
        self.description = description
        self.argumentHint = argumentHint
    }
}

public struct AgentCommands: Codable, Hashable, Sendable {
    public var agentId: String
    public var commands: [Command]
    public init(agentId: String, commands: [Command]) {
        self.agentId = agentId
        self.commands = commands
    }
}

public struct McpServer: Codable, Hashable, Sendable {
    public struct Status: OpenString {
        public let rawValue: String
        public init(rawValue: String) { self.rawValue = rawValue }
        public static let connected: Self = "connected"
        public static let pending: Self = "pending"
        public static let needsAuth: Self = "needsAuth"
        public static let failed: Self = "failed"
        public static let disabled: Self = "disabled"
    }

    public var name: String
    public var status: Status
    public var scope: String?

    public init(name: String, status: Status, scope: String? = nil) {
        self.name = name
        self.status = status
        self.scope = scope
    }
}

public struct AgentMcp: Codable, Hashable, Sendable {
    public var agentId: String
    public var servers: [McpServer]
    public var authUrl: String?
    public init(agentId: String, servers: [McpServer], authUrl: String? = nil) {
        self.agentId = agentId
        self.servers = servers
        self.authUrl = authUrl
    }
}

public struct AgentBtwAnswer: Codable, Hashable, Sendable {
    public var agentId: String
    public var answer: String
    public init(agentId: String, answer: String) {
        self.agentId = agentId
        self.answer = answer
    }
}

/// One new or changed timeline item, pushed live.
public struct AgentStream: Codable, Hashable, Sendable {
    public var agentId: String
    public var epoch: Int
    public var item: TimelineItem
    public init(agentId: String, epoch: Int, item: TimelineItem) {
        self.agentId = agentId
        self.epoch = epoch
        self.item = item
    }
}

public struct AgentTimeline: Codable, Hashable, Sendable {
    public var agentId: String
    public var epoch: Int
    public var items: [TimelineItem]
    /// With `beforeSeq`, exact: true only when an item older than the oldest returned exists.
    public var hasOlder: Bool
    public var maxSeq: Int
    public init(agentId: String, epoch: Int, items: [TimelineItem], hasOlder: Bool, maxSeq: Int) {
        self.agentId = agentId
        self.epoch = epoch
        self.items = items
        self.hasOlder = hasOlder
        self.maxSeq = maxSeq
    }
}

/// `permission.request {request}`: an ask the agent waits on.
public struct PermissionAsk: Codable, Hashable, Sendable {
    public var request: PermissionRequest
    public init(request: PermissionRequest) { self.request = request }
}

public struct PermissionResolved: Codable, Hashable, Sendable {
    public var requestId: String
    public var decision: Decision
    public init(requestId: String, decision: Decision) {
        self.requestId = requestId
        self.decision = decision
    }
}

/// No attempt named the agent's session from its prompt.
public struct NamingFailed: Codable, Hashable, Sendable {
    public var agentId: String
    public init(agentId: String) { self.agentId = agentId }
}

extension ClientMessage {
    /// The many verbs that name only an agent.
    public struct AgentRef: Codable, Hashable, Sendable {
        public var agentId: String
        public init(agentId: String) { self.agentId = agentId }
    }

    public struct AgentText: Codable, Hashable, Sendable {
        public var agentId: String
        public var text: String
        public init(agentId: String, text: String) {
            self.agentId = agentId
            self.text = text
        }
    }

    public struct AgentMcp: Codable, Hashable, Sendable {
        public struct Action: OpenString {
            public let rawValue: String
            public init(rawValue: String) { self.rawValue = rawValue }
            public static let auth: Self = "auth"
            public static let reconnect: Self = "reconnect"
            public static let enable: Self = "enable"
        }

        public var agentId: String
        public var action: Action?
        public var server: String?
        public init(agentId: String, action: Action? = nil, server: String? = nil) {
            self.agentId = agentId
            self.action = action
            self.server = server
        }
    }

    public struct AgentSettings: Codable, Hashable, Sendable {
        public var agentId: String
        public var model: String?
        public var effort: String?
        /// ask, edits, auto or full; never "settings".
        public var access: String?
        public init(agentId: String, model: String? = nil, effort: String? = nil, access: String? = nil) {
            self.agentId = agentId
            self.model = model
            self.effort = effort
            self.access = access
        }
    }

    public struct AgentPin: Codable, Hashable, Sendable {
        public var agentId: String
        public var pinned: Bool
        public init(agentId: String, pinned: Bool) {
            self.agentId = agentId
            self.pinned = pinned
        }
    }

    /// `agent.view` and `agent.seen`.
    public struct AgentIds: Codable, Hashable, Sendable {
        public var agentIds: [String]
        public init(agentIds: [String]) { self.agentIds = agentIds }
    }

    /// `beforeSeq` and `sinceSeq` together are malformed.
    public struct AgentTimeline: Codable, Hashable, Sendable {
        public var agentId: String
        public var sinceSeq: Int?
        public var beforeSeq: Int?
        public var limit: Int?
        public init(agentId: String, sinceSeq: Int? = nil, beforeSeq: Int? = nil, limit: Int? = nil) {
            self.agentId = agentId
            self.sinceSeq = sinceSeq
            self.beforeSeq = beforeSeq
            self.limit = limit
        }
    }

    public struct PermissionResolve: Codable, Hashable, Sendable {
        public var requestId: String
        public var decision: Decision
        public var option: String?
        public var message: String?
        public var answers: [String: String]?
        public init(requestId: String, decision: Decision, option: String? = nil, message: String? = nil, answers: [String: String]? = nil) {
            self.requestId = requestId
            self.decision = decision
            self.option = option
            self.message = message
            self.answers = answers
        }
    }

    /// Switch a chat-only agent's account.
    public struct AgentAccount: Codable, Hashable, Sendable {
        public var agentId: String
        public var account: String
        public init(agentId: String, account: String) {
            self.agentId = agentId
            self.account = account
        }
    }
}
