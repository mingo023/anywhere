/// How much an agent may do without asking. "settings" leaves it to the agent's own settings.
public struct Access: OpenString {
    public let rawValue: String
    public init(rawValue: String) { self.rawValue = rawValue }
    public static let settings: Self = "settings"
    public static let ask: Self = "ask"
    public static let edits: Self = "edits"
    public static let auto: Self = "auto"
    public static let full: Self = "full"
}

public struct ProviderId: OpenString {
    public let rawValue: String
    public init(rawValue: String) { self.rawValue = rawValue }
    public static let claude: Self = "claude"
    public static let codex: Self = "codex"
}

public struct NewWorktree: Codable, Hashable, Sendable {
    public var name: String?
    public var base: String?
    /// Opens an existing branch instead of naming a new one (open.v1).
    public var branch: String?
    public var pr: String?
    public var copy: Bool?
    public var setup: Bool?
    public var autoName: Bool?

    public init(name: String? = nil, base: String? = nil, branch: String? = nil, pr: String? = nil, copy: Bool? = nil, setup: Bool? = nil, autoName: Bool? = nil) {
        self.name = name
        self.base = base
        self.branch = branch
        self.pr = pr
        self.copy = copy
        self.setup = setup
        self.autoName = autoName
    }
}

/// Where a new agent runs.
public enum Checkout: Codable, Hashable, Sendable {
    case worktree(String)
    case new(NewWorktree)
    case local(String)

    enum Keys: String, CodingKey {
        case worktree, new, local
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: Keys.self)
        if let path = try c.decodeIfPresent(String.self, forKey: .worktree) {
            self = .worktree(path)
        } else if let new = try c.decodeIfPresent(NewWorktree.self, forKey: .new) {
            self = .new(new)
        } else if let local = try c.decodeIfPresent(String.self, forKey: .local) {
            self = .local(local)
        } else {
            throw malformed(decoder, "checkout needs worktree, new or local")
        }
    }

    public func encode(to encoder: any Encoder) throws {
        var c = encoder.container(keyedBy: Keys.self)
        switch self {
        case .worktree(let path): try c.encode(path, forKey: .worktree)
        case .new(let new): try c.encode(new, forKey: .new)
        case .local(let id): try c.encode(id, forKey: .local)
        }
    }
}

public struct LaunchSpec: Codable, Hashable, Sendable {
    public var project: String
    public var checkout: Checkout
    public var provider: ProviderId
    public var model: String?
    public var effort: String?
    public var access: Access
    public var plan: Bool
    public var prompt: String?
    /// A provider session to fork (fork.v1).
    public var fork: String?
    public var account: String?
    /// Run without its TUI: only the chat drives it.
    public var chat: Bool?

    public init(
        project: String, checkout: Checkout, provider: ProviderId, model: String? = nil, effort: String? = nil, access: Access,
        plan: Bool, prompt: String? = nil, fork: String? = nil, account: String? = nil, chat: Bool? = nil
    ) {
        self.project = project
        self.checkout = checkout
        self.provider = provider
        self.model = model
        self.effort = effort
        self.access = access
        self.plan = plan
        self.prompt = prompt
        self.fork = fork
        self.account = account
        self.chat = chat
    }
}

/// What a provider's chat can do.
public struct ChatCaps: Codable, Hashable, Sendable {
    public var interrupt: Bool
    public var compact: Bool
    public var sendWhileWorking: Bool
    public var subAgents: Bool
    public var sendNow: Bool?

    public init(interrupt: Bool, compact: Bool, sendWhileWorking: Bool, subAgents: Bool, sendNow: Bool? = nil) {
        self.interrupt = interrupt
        self.compact = compact
        self.sendWhileWorking = sendWhileWorking
        self.subAgents = subAgents
        self.sendNow = sendNow
    }
}

public struct ProviderInfo: Codable, Hashable, Sendable, Identifiable {
    public var id: String
    public var available: Bool
    public var efforts: [String]
    public var plan: Bool
    /// Older pocketds don't send it.
    public var chat: ChatCaps?

    public init(id: String, available: Bool, efforts: [String], plan: Bool, chat: ChatCaps? = nil) {
        self.id = id
        self.available = available
        self.efforts = efforts
        self.plan = plan
        self.chat = chat
    }
}

public struct AgentProviders: Codable, Hashable, Sendable {
    public var providers: [ProviderInfo]
    public var maxAccess: String
    public var phoneMaxAccess: String

    public init(providers: [ProviderInfo], maxAccess: String, phoneMaxAccess: String) {
        self.providers = providers
        self.maxAccess = maxAccess
        self.phoneMaxAccess = phoneMaxAccess
    }
}

/// The terminal an `agent.create` opened, before its agent starts.
public struct AgentCreating: Codable, Hashable, Sendable {
    public var requestId: String
    public var terminalId: String
    public var cwd: String
    public var setup: Bool

    public init(requestId: String, terminalId: String, cwd: String, setup: Bool) {
        self.requestId = requestId
        self.terminalId = terminalId
        self.cwd = cwd
        self.setup = setup
    }
}

/// A step of making a worktree or launching into it: prepare, verify, fetch, worktree, copy, setup.
public struct LaunchProgress: Codable, Hashable, Sendable {
    public var requestId: String
    public var step: String
    public var note: String?

    public init(requestId: String, step: String, note: String? = nil) {
        self.requestId = requestId
        self.step = step
        self.note = note
    }
}

public struct AgentCreated: Codable, Hashable, Sendable {
    public var requestId: String
    public var agentId: String
    public var terminalId: String

    public init(requestId: String, agentId: String, terminalId: String) {
        self.requestId = requestId
        self.agentId = agentId
        self.terminalId = terminalId
    }
}

extension ClientMessage {
    public struct AgentCreate: Codable, Hashable, Sendable {
        /// 1 to 64 characters; names the launch in its progress replies.
        public var requestId: String
        public var spec: LaunchSpec
        public init(requestId: String, spec: LaunchSpec) {
            self.requestId = requestId
            self.spec = spec
        }
    }

    /// `phone.maxAccess` (ask, edits, auto) or `phone.terminals` (on, off), owner only.
    public struct ConfigSet: Codable, Hashable, Sendable {
        public var key: String
        public var value: String
        public init(key: String, value: String) {
            self.key = key
            self.value = value
        }
    }
}
