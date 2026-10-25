public struct DiffLine: Codable, Hashable, Sendable {
    public struct Kind: OpenString {
        public let rawValue: String
        public init(rawValue: String) { self.rawValue = rawValue }
        public static let add: Self = "add"
        public static let del: Self = "del"
        public static let context: Self = "context"
    }

    public var number: Int?
    public var kind: Kind
    public var text: String

    public init(number: Int? = nil, kind: Kind, text: String) {
        self.number = number
        self.kind = kind
        self.text = text
    }
}

public struct FileDiff: Codable, Hashable, Sendable {
    public var lines: [DiffLine]
    public var additions: Int
    public var deletions: Int
    /// A write with no prior contents to compare against, so every line reads as added.
    public var contentOnly: Bool?

    public init(lines: [DiffLine], additions: Int, deletions: Int, contentOnly: Bool? = nil) {
        self.lines = lines
        self.additions = additions
        self.deletions = deletions
        self.contentOnly = contentOnly
    }
}

/// What a tool call acts on, keyed by `kind`.
public enum ToolDetail: Codable, Hashable, Sendable {
    case shell(command: String, description: String?)
    case read(path: String)
    case edit(path: String, diff: FileDiff?)
    case write(path: String, diff: FileDiff?)
    case search(query: String, path: String?)
    /// A sub-agent: `tokens` is the context it last answered from.
    case task(description: String, model: String?, tokens: Int?)
    /// `description` says what the call is about in words: the task a TaskStop or TaskOutput acts on.
    case other(name: String, input: JSONValue, description: String?)
    case unknown(kind: String)

    enum Keys: String, CodingKey {
        case kind, command, description, path, diff, query, model, tokens, name, input
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: Keys.self)
        let kind = try c.decode(String.self, forKey: .kind)
        switch kind {
        case "shell":
            self = .shell(command: try c.decode(String.self, forKey: .command), description: try c.decodeIfPresent(String.self, forKey: .description))
        case "read":
            self = .read(path: try c.decode(String.self, forKey: .path))
        case "edit":
            self = .edit(path: try c.decode(String.self, forKey: .path), diff: try c.decodeIfPresent(FileDiff.self, forKey: .diff))
        case "write":
            self = .write(path: try c.decode(String.self, forKey: .path), diff: try c.decodeIfPresent(FileDiff.self, forKey: .diff))
        case "search":
            self = .search(query: try c.decode(String.self, forKey: .query), path: try c.decodeIfPresent(String.self, forKey: .path))
        case "task":
            self = .task(
                description: try c.decode(String.self, forKey: .description),
                model: try c.decodeIfPresent(String.self, forKey: .model),
                tokens: try c.decodeIfPresent(Int.self, forKey: .tokens))
        case "other":
            self = .other(
                name: try c.decode(String.self, forKey: .name),
                input: try c.decodeIfPresent(JSONValue.self, forKey: .input) ?? .null,
                description: try c.decodeIfPresent(String.self, forKey: .description))
        default:
            self = .unknown(kind: kind)
        }
    }

    public func encode(to encoder: any Encoder) throws {
        var c = encoder.container(keyedBy: Keys.self)
        try c.encode(kind, forKey: .kind)
        switch self {
        case .shell(let command, let description):
            try c.encode(command, forKey: .command)
            try c.encodeIfPresent(description, forKey: .description)
        case .read(let path):
            try c.encode(path, forKey: .path)
        case .edit(let path, let diff), .write(let path, let diff):
            try c.encode(path, forKey: .path)
            try c.encodeIfPresent(diff, forKey: .diff)
        case .search(let query, let path):
            try c.encode(query, forKey: .query)
            try c.encodeIfPresent(path, forKey: .path)
        case .task(let description, let model, let tokens):
            try c.encode(description, forKey: .description)
            try c.encodeIfPresent(model, forKey: .model)
            try c.encodeIfPresent(tokens, forKey: .tokens)
        case .other(let name, let input, let description):
            try c.encode(name, forKey: .name)
            try c.encode(input, forKey: .input)
            try c.encodeIfPresent(description, forKey: .description)
        case .unknown:
            break
        }
    }

    public var kind: String {
        switch self {
        case .shell: "shell"
        case .read: "read"
        case .edit: "edit"
        case .write: "write"
        case .search: "search"
        case .task: "task"
        case .other: "other"
        case .unknown(let kind): kind
        }
    }

    /// The file a read, edit or write acts on.
    public var path: String? {
        switch self {
        case .read(let path), .edit(let path, _), .write(let path, _): path
        default: nil
        }
    }
}

public struct ToolCall: Codable, Hashable, Sendable {
    public struct Status: OpenString {
        public let rawValue: String
        public init(rawValue: String) { self.rawValue = rawValue }
        public static let running: Self = "running"
        public static let ok: Self = "ok"
        public static let error: Self = "error"
    }

    public var toolUseId: String
    public var name: String
    public var detail: ToolDetail
    public var status: Status
    public var output: String?
    public var durationMs: Int?

    public init(toolUseId: String, name: String, detail: ToolDetail, status: Status, output: String? = nil, durationMs: Int? = nil) {
        self.toolUseId = toolUseId
        self.name = name
        self.detail = detail
        self.status = status
        self.output = output
        self.durationMs = durationMs
    }
}

public struct TaskItem: Codable, Hashable, Sendable {
    public struct Status: OpenString {
        public let rawValue: String
        public init(rawValue: String) { self.rawValue = rawValue }
        public static let pending: Self = "pending"
        public static let inProgress: Self = "in_progress"
        public static let completed: Self = "completed"
    }

    public var text: String
    public var status: Status

    public init(text: String, status: Status) {
        self.text = text
        self.status = status
    }
}

public struct TurnUsage: Codable, Hashable, Sendable {
    public var inputTokens: Int
    public var outputTokens: Int
    public var cacheReadTokens: Int?

    public init(inputTokens: Int, outputTokens: Int, cacheReadTokens: Int? = nil) {
        self.inputTokens = inputTokens
        self.outputTokens = outputTokens
        self.cacheReadTokens = cacheReadTokens
    }
}

/// What a timeline item holds, keyed by `kind`.
public enum TimelineBody: Hashable, Sendable {
    case user(text: String)
    case assistant(text: String)
    case thinking(text: String)
    case tool(ToolCall)
    case tasks([TaskItem])
    case plan(text: String)
    case compact(trigger: String)
    case result(ok: Bool, durationMs: Int, error: String?, usage: TurnUsage?)
    /// Why the agent went on without a prompt, or an error it went on after.
    case notice(text: String?, error: String?)
    case unknown(kind: String)

    public var kind: String {
        switch self {
        case .user: "user"
        case .assistant: "assistant"
        case .thinking: "thinking"
        case .tool: "tool"
        case .tasks: "tasks"
        case .plan: "plan"
        case .compact: "compact"
        case .result: "result"
        case .notice: "notice"
        case .unknown(let kind): kind
        }
    }
}

/// One entry of an agent's timeline. `seq` orders items within an `epoch`.
public struct TimelineItem: Codable, Hashable, Sendable, Identifiable {
    public var id: String
    public var seq: Int
    /// Unix milliseconds.
    public var ts: Int
    /// The tool call that spawned the sub-agent this item belongs to.
    public var parentToolUseId: String?
    public var body: TimelineBody

    public init(id: String, seq: Int, ts: Int, parentToolUseId: String? = nil, body: TimelineBody) {
        self.id = id
        self.seq = seq
        self.ts = ts
        self.parentToolUseId = parentToolUseId
        self.body = body
    }

    enum Keys: String, CodingKey {
        case kind, id, seq, ts, parentToolUseId, text, call, items, trigger, ok, durationMs, error, usage
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: Keys.self)
        id = try c.decode(String.self, forKey: .id)
        seq = try c.decode(Int.self, forKey: .seq)
        ts = try c.decode(Int.self, forKey: .ts)
        parentToolUseId = try c.decodeIfPresent(String.self, forKey: .parentToolUseId)
        let kind = try c.decode(String.self, forKey: .kind)
        let text = { try c.decode(String.self, forKey: .text) }
        switch kind {
        case "user": body = .user(text: try text())
        case "assistant": body = .assistant(text: try text())
        case "thinking": body = .thinking(text: try text())
        case "plan": body = .plan(text: try text())
        case "tool": body = .tool(try c.decode(ToolCall.self, forKey: .call))
        case "tasks": body = .tasks(try c.decode([TaskItem].self, forKey: .items))
        case "compact": body = .compact(trigger: try c.decode(String.self, forKey: .trigger))
        case "result":
            body = .result(
                ok: try c.decode(Bool.self, forKey: .ok),
                durationMs: try c.decode(Int.self, forKey: .durationMs),
                error: try c.decodeIfPresent(String.self, forKey: .error),
                usage: try c.decodeIfPresent(TurnUsage.self, forKey: .usage))
        case "notice":
            body = .notice(text: try c.decodeIfPresent(String.self, forKey: .text), error: try c.decodeIfPresent(String.self, forKey: .error))
        default: body = .unknown(kind: kind)
        }
    }

    public func encode(to encoder: any Encoder) throws {
        var c = encoder.container(keyedBy: Keys.self)
        try c.encode(body.kind, forKey: .kind)
        try c.encode(id, forKey: .id)
        try c.encode(seq, forKey: .seq)
        try c.encode(ts, forKey: .ts)
        try c.encodeIfPresent(parentToolUseId, forKey: .parentToolUseId)
        switch body {
        case .user(let text), .assistant(let text), .thinking(let text), .plan(let text):
            try c.encode(text, forKey: .text)
        case .tool(let call): try c.encode(call, forKey: .call)
        case .tasks(let items): try c.encode(items, forKey: .items)
        case .compact(let trigger): try c.encode(trigger, forKey: .trigger)
        case .result(let ok, let durationMs, let error, let usage):
            try c.encode(ok, forKey: .ok)
            try c.encode(durationMs, forKey: .durationMs)
            try c.encodeIfPresent(error, forKey: .error)
            try c.encodeIfPresent(usage, forKey: .usage)
        case .notice(let text, let error):
            try c.encodeIfPresent(text, forKey: .text)
            try c.encodeIfPresent(error, forKey: .error)
        case .unknown: break
        }
    }
}

public struct Decision: OpenString {
    public let rawValue: String
    public init(rawValue: String) { self.rawValue = rawValue }
    public static let allow: Self = "allow"
    public static let deny: Self = "deny"
}

/// An allow variant offered beside plain allow and deny.
public struct PermissionOption: Codable, Hashable, Sendable {
    public var id: String
    public var label: String

    public init(id: String, label: String) {
        self.id = id
        self.label = label
    }
}

public struct PermissionRequest: Codable, Hashable, Sendable, Identifiable {
    public var requestId: String
    public var agentId: String
    public var toolName: String
    public var detail: ToolDetail
    public var options: [PermissionOption]?
    /// Set when a deny can carry a message back to the agent.
    public var feedback: Bool?

    public var id: String { requestId }

    public init(requestId: String, agentId: String, toolName: String, detail: ToolDetail, options: [PermissionOption]? = nil, feedback: Bool? = nil) {
        self.requestId = requestId
        self.agentId = agentId
        self.toolName = toolName
        self.detail = detail
        self.options = options
        self.feedback = feedback
    }
}
