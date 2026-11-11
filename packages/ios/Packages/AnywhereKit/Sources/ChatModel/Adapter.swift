import Domain
import Wire

/// What an agent's chat can do; the view hides what it can't.
public struct Capabilities: Hashable, Sendable {
    public var interrupt: Bool
    public var compact: Bool
    /// A prompt sent mid-turn waits in the agent's own queue instead of being lost.
    public var sendWhileWorking: Bool
    /// Its spawn tool calls carry a nested transcript.
    public var subAgents: Bool
    /// It can take what was sent into its turn before its current step ends. Only pocketd says so: an older one can't.
    public var sendNow: Bool

    public init(interrupt: Bool = false, compact: Bool = false, sendWhileWorking: Bool = false, subAgents: Bool = false, sendNow: Bool = false) {
        self.interrupt = interrupt
        self.compact = compact
        self.sendWhileWorking = sendWhileWorking
        self.subAgents = subAgents
        self.sendNow = sendNow
    }

    public init(_ c: ChatCaps) {
        self.init(interrupt: c.interrupt, compact: c.compact, sendWhileWorking: c.sendWhileWorking, subAgents: c.subAgents, sendNow: c.sendNow ?? false)
    }

    /// Whether the composer's button may do `action` for this agent.
    public func allows(_ action: Action) -> Bool {
        switch action {
        case .stop: interrupt
        case .steer, .queue: sendWhileWorking
        case .send: true
        case .stopping: false
        }
    }
}

/// How the chat treats one provider's agent: what it can do and which commands it runs itself.
public struct Adapter: Hashable, Sendable {
    /// Nil for a provider this build doesn't know.
    public let agent: AgentName?

    /// The adapter for a pocketd agent, by its summary's `provider`; a generic one for a provider this build doesn't know.
    public static func `for`(_ provider: String) -> Adapter {
        Adapter(agent: AgentName(rawValue: provider))
    }

    /// What pocketd drives the agent's chat over.
    private enum Driver { case claude, codex, pi, acp, other }

    private var driver: Driver {
        switch agent {
        case .claude: .claude
        case .codex: .codex
        case .pi, .omp: .pi
        case .cursor, .grok, .opencode, .fx, .hermes, .antigravity: .acp
        case nil: .other
        }
    }

    public var capabilities: Capabilities {
        switch driver {
        case .claude, .other: Capabilities(interrupt: true, compact: true, sendWhileWorking: true, subAgents: true)
        case .codex, .pi: Capabilities(interrupt: true, compact: true, sendWhileWorking: true)
        // A pocketd that says no caps compacts no ACP chat.
        case .acp: Capabilities(interrupt: true, sendWhileWorking: true)
        }
    }

    /// What the chat does with command `name` given `arg`. One that opens a panel in the agent's terminal is
    /// unsupported: the chat can't show it, and the next send would land in its keys.
    public func command(_ name: String, _ arg: String) -> Kind {
        switch driver {
        case .claude:
            switch name {
            case "model": return .local(.model)
            case "effort": return .local(.effort)
            case "compact" where arg.isEmpty: return .local(.compact)
            case "new": return .local(.new)
            case "btw": return .local(.btw)
            case "mcp": return .local(.mcp)
            case "clear", "reset", "resume": return .unsupported
            case "add-dir" where arg.isEmpty: return .unsupported
            case _ where Self.claudePanels.contains(name): return .unsupported
            default: return .send
            }
        case .codex:
            switch name {
            case "model": return .local(.model)
            case "effort": return .local(.effort)
            case "compact": return .local(.compact)
            case "new": return .local(.new)
            case "mcp": return .local(.mcp)
            case "clear": return .unsupported
            case _ where Self.codexTuiOnly.contains(name): return .unsupported
            default: return .send
            }
        case .other:
            return .send
        // Over Pi's RPC or ACP a command of the agent's TUI would reach the model as a prompt.
        case .pi, .acp:
            switch name {
            case "model": return .local(.model)
            case "effort": return .local(.effort)
            case "new": return .local(.new)
            case "thinking" where driver == .pi: return .local(.effort)
            case "compact" where driver == .pi || compactsOverACP: return .local(.compact)
            case _ where tuiOnly.contains(name): return .unsupported
            default: return .send
            }
        }
    }

    /// The commands the `/` menu offers before the agent's CLI lists its own.
    public var builtins: [Builtin] {
        switch driver {
        case .claude: Self.claudeBuiltins
        case .codex: Self.codexBuiltins
        case .pi: Self.piBuiltins
        case .acp: compactsOverACP ? Self.compactingAcpBuiltins : Self.acpBuiltins
        case .other: []
        }
    }

    /// Whether `/goal pause` holds the agent's goal without dropping it.
    public var pausesGoal: Bool { agent == .codex }

    static let claudeBuiltins = [
        Builtin(name: "model", hint: "[model]", description: "Set the model"),
        Builtin(name: "effort", hint: "[level]", description: "Set the reasoning effort"),
        Builtin(name: "compact", hint: "[instructions]", description: "Summarize the conversation to free up context"),
        Builtin(name: "btw", hint: "[question]", description: "Ask a quick side question without interrupting"),
        Builtin(name: "goal", hint: "[<condition> | clear]", description: "Keep working until the condition is met"),
        Builtin(name: "mcp", hint: "", description: "Find an MCP server for this message"),
    ]

    /// Claude's commands that open a panel, by name and alias.
    static let claudePanels: Set = [
        "agents", "bug", "checkpoint", "checkup", "config", "cost", "diff", "doctor", "export", "feedback", "help", "hooks", "ide", "login", "logout",
        "memory", "permissions", "plugin", "privacy-settings", "rewind", "settings", "skills", "stats", "status", "tasks", "terminal-setup",
        "theme", "usage",
    ]

    /// pocketd turns the goal into `thread/goal/*` calls and the rest into a skill or a prompt.
    static let codexBuiltins = [
        Builtin(name: "goal", hint: "[<objective> | pause | resume | clear]", description: "Set a goal to keep working toward"),
        Builtin(name: "compact", hint: "", description: "Summarize the conversation to free up context"),
        Builtin(name: "model", hint: "[model]", description: "Set the model"),
        Builtin(name: "effort", hint: "[level]", description: "Set the reasoning effort"),
        Builtin(name: "new", hint: "", description: "Start a new chat"),
        Builtin(name: "mcp", hint: "", description: "Find an MCP server for this message"),
    ]

    /// Codex's commands that only its TUI runs.
    static let codexTuiOnly: Set = [
        "approvals", "permissions", "experimental", "skills", "review", "rename", "resume", "fork", "init", "plan", "collab", "agent", "diff",
        "mention", "status", "statusline", "theme", "apps", "logout", "quit", "exit", "feedback", "ps", "personality", "undo",
    ]

    static let acpBuiltins = [
        Builtin(name: "model", hint: "[model]", description: "Set the model"),
        Builtin(name: "new", hint: "", description: "Start a new chat"),
    ]

    static let compactingAcpBuiltins = [
        Builtin(name: "model", hint: "[model]", description: "Set the model"),
        Builtin(name: "compact", hint: "", description: "Summarize the conversation to free up context"),
        Builtin(name: "new", hint: "", description: "Start a new chat"),
    ]

    /// pocketd compacts these agents' ACP chats when asked, so `/compact` does that rather than reach the model.
    private var compactsOverACP: Bool { agent == .grok || agent == .opencode }

    static let piBuiltins = [
        Builtin(name: "model", hint: "[model]", description: "Set the model"),
        Builtin(name: "thinking", hint: "[level]", description: "Set the thinking level"),
        Builtin(name: "compact", hint: "", description: "Summarize the conversation to free up context"),
        Builtin(name: "new", hint: "", description: "Start a new chat"),
    ]

    /// The commands only the agent's TUI runs. None for omp, which lists what its RPC runs.
    private var tuiOnly: Set<String> {
        switch agent {
        case .pi: Self.piTuiOnly
        case .opencode: Self.opencodeTuiOnly
        case .hermes: Self.hermesTuiOnly
        case .claude, .codex, .cursor, .grok, .omp, .fx, .antigravity, nil: []
        }
    }

    /// OpenCode 1.19's TUI commands, by name and alias; its ACP server lists only skills, `init` and `review`.
    static let opencodeTuiOnly: Set = [
        "agents", "clear", "compact", "connect", "continue", "copy", "debug", "diff", "editor", "exit", "export", "fork", "help", "mcps", "mo",
        "models", "move", "org", "orgs", "q", "quit", "redo", "rename", "resume", "sessions", "skills", "slash", "status", "summarize",
        "switch-org", "themes", "thinking", "timeline", "timestamps", "toggle-thinking", "toggle-timestamps", "undo", "update", "variants",
        "warp", "workspaces",
    ]

    /// Pi's built-in commands, which only its interactive mode handles (`BUILTIN_SLASH_COMMANDS`), and `llama`, which its
    /// own extension adds and runs only there.
    static let piTuiOnly: Set = [
        "settings", "tree", "scoped-models", "export", "import", "share", "bug", "copy", "name", "session", "changelog", "hotkeys", "fork",
        "clone", "trust", "login", "logout", "resume", "reload", "quit", "llama",
    ]

    /// The commands Hermes marks `cli_only`, by name and alias, but `tools`, which its ACP server answers.
    static let hermesTuiOnly: Set = [
        "clear", "redraw", "history", "prompt", "compose", "handoff", "worktree", "snapshot", "snap", "export", "import", "journey", "learning",
        "memory-graph", "config", "statusbar", "sb", "battery", "timestamps", "ts", "verbose", "focus", "skin", "indicator", "wake", "toolsets",
        "skills", "pet", "hatch", "generate-pet", "cron", "reload", "browser", "plugins", "palette", "subscription", "upgrade", "platforms",
        "gateway", "copy", "paste", "image", "quit", "exit",
    ]
}

/// What `provider`'s chat can do: what pocketd `said`, else its adapter's own, for a pocketd older than `chat`.
public func capabilities(_ provider: String, _ said: ChatCaps?) -> Capabilities {
    said.map(Capabilities.init) ?? Adapter.for(provider).capabilities
}
