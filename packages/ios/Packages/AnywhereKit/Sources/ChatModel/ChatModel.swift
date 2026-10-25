import Wire

/// One agent's conversation, oldest turn first.
public struct Transcript: Hashable, Sendable {
    public var turns: [Turn]

    public init(turns: [Turn] = []) { self.turns = turns }
}

/// A prompt and everything the agent did for it, up to its end.
public struct Turn: Hashable, Sendable {
    /// The id of the turn's first item.
    public var id: String
    /// Nil for a turn the agent began itself (history before the first fetched prompt, an auto compact).
    public var prompt: Message?
    public var blocks: [Block]
    /// Nil while the agent is still on it.
    public var end: End?

    public init(id: String, prompt: Message? = nil, blocks: [Block] = [], end: End? = nil) {
        self.id = id
        self.prompt = prompt
        self.blocks = blocks
        self.end = end
    }
}

public struct Message: Hashable, Sendable {
    public var id: String
    /// Unix ms.
    public var at: Int
    public var text: String

    public init(id: String, at: Int, text: String) {
        self.id = id
        self.at = at
        self.text = text
    }
}

public enum Block: Hashable, Sendable {
    case text(Message)
    case thinking(Message)
    case tool(Call)
    case todos(id: String, todos: [Todo])
    /// A plan-mode plan, markdown.
    case plan(Message)
    case compacted(id: String, auto: Bool)
    /// Why the agent went on without a prompt, as "Stop hook: tests fail", or an `error` it went on after.
    case notice(Message, error: Bool)

    public var call: Call? {
        if case .tool(let call) = self { call } else { nil }
    }
}

/// A tool call as the chat shows it.
public struct Call: Hashable, Sendable {
    /// The agent's tool-use id; sub-agent items point at it.
    public var id: String
    public var name: String
    public var kind: ToolKind
    public var status: CallStatus
    public var output: String?
    public var durationMs: Int?
    public var at: Int

    public init(id: String, name: String, kind: ToolKind, status: CallStatus, output: String? = nil, durationMs: Int? = nil, at: Int = 0) {
        self.id = id
        self.name = name
        self.kind = kind
        self.status = status
        self.output = output
        self.durationMs = durationMs
        self.at = at
    }

    public var agent: SubAgent? {
        if case .agent(let sub) = kind { sub } else { nil }
    }
}

public enum CallStatus: Hashable, Sendable {
    case running, ok, failed
}

public enum ToolKind: Hashable, Sendable {
    case shell(command: String)
    case read(path: String)
    case edit(path: String, diff: FileDiff?)
    case write(path: String, diff: FileDiff?)
    case search(query: String)
    case fetch(url: String)
    case question([Question])
    case agent(SubAgent)
    /// Any other tool; `input` is its JSON input, pretty-printed, and `description` what it is about in words, when known.
    case other(input: String, description: String)
}

public struct SubAgent: Hashable, Sendable {
    public var description: String
    /// Empty until the backend reports sub-agent items (`Capabilities.subAgents`).
    public var transcript: Transcript
    /// The model it runs on, an alias until it first replies; empty when unknown.
    public var model: String
    /// The context it last answered from; 0 until it first replies.
    public var tokens: Int

    public init(description: String, transcript: Transcript = Transcript(), model: String = "", tokens: Int = 0) {
        self.description = description
        self.transcript = transcript
        self.model = model
        self.tokens = tokens
    }
}

public struct Todo: Hashable, Sendable {
    public enum Status: Hashable, Sendable {
        case pending, active, done
    }

    public var text: String
    public var status: Status

    public init(text: String, status: Status) {
        self.text = text
        self.status = status
    }
}

public struct Question: Hashable, Sendable {
    public var header: String
    public var question: String
    public var options: [Choice]
    public var multi: Bool

    public init(header: String = "", question: String, options: [Choice] = [], multi: Bool = false) {
        self.header = header
        self.question = question
        self.options = options
        self.multi = multi
    }
}

public struct Choice: Hashable, Sendable {
    public var label: String
    public var description: String

    public init(label: String, description: String = "") {
        self.label = label
        self.description = description
    }
}

public enum End: Hashable, Sendable {
    case done(durationMs: Int, usage: Usage?)
    case failed(error: String, durationMs: Int)

    public var durationMs: Int {
        switch self {
        case .done(let ms, _), .failed(_, let ms): ms
        }
    }
}

public struct Usage: Hashable, Sendable {
    public var input: Int
    public var output: Int
    public var cacheRead: Int

    public init(input: Int, output: Int, cacheRead: Int) {
        self.input = input
        self.output = output
        self.cacheRead = cacheRead
    }
}

/// A pending ask for the user: a tool waiting for permission, or questions to answer.
public struct Ask: Hashable, Sendable {
    public var requestId: String
    public var tool: ToolKind
    public var toolName: String
    public var questions: [Question]

    public init(requestId: String, tool: ToolKind, toolName: String, questions: [Question]) {
        self.requestId = requestId
        self.tool = tool
        self.toolName = toolName
        self.questions = questions
    }
}
