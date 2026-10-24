import Wire

/// What stacks over the composer box and sits in its bar: trays, the goal strip, the pills and the context ring.

/// Sub-agents and commands left running past their turn.
public enum Background {
    /// A sub-agent's newest tool call; none for a command.
    static func latest(_ call: Call) -> Call? {
        call.agent.flatMap(Agent.latest)
    }

    /// What a task does now: a sub-agent's newest step, "Starting…" before it took one; none for a command.
    public static func doing(_ call: Call) -> String? {
        if let c = latest(call) { return Agent.step(c) }
        return call.agent != nil ? "Starting…" : nil
    }

    /// The newest step any task took: the folded tray's one line.
    public static func newest(_ tasks: [Call]) -> String? {
        tasks.compactMap(latest).max { $0.at < $1.at }.map(Agent.step)
    }

    public static func name(_ call: Call) -> String {
        call.agent.map(Agent.name) ?? Agent.step(call)
    }

    /// Whether the tray offers Stop: a chat's own agent stops the sub-agents it left in the background, where a terminal
    /// agent's Esc doesn't reach them. pocketd leaves background commands running, and while a turn runs the composer's Stop
    /// ends it and them, so the tray offers it only for sub-agents outliving their turn.
    public static func stops(chatOnly: Bool, interrupt: Bool, busy: Bool, _ tasks: [Call]) -> Bool {
        chatOnly && interrupt && !busy && tasks.contains { $0.agent != nil }
    }
}

/// Sends held behind the agent's turn.
public enum Queue {
    /// The sends held behind the agent's turn; the transcript shows the rest.
    public static func waiting(_ sends: [Pending]) -> [Pending] {
        sends.filter { $0.delivery == .queued }
    }

    /// The composer's text once a held send comes back to it: under what was already typed, which it doesn't replace.
    public static func editing(_ draft: String, _ held: String) -> String {
        draft.allSatisfy(\.isWhitespace) ? held : "\(draft)\n\n\(held)"
    }
}

/// An agent's `/goal`.
public enum GoalStrip {
    public static func statusLabel(_ status: String) -> String {
        switch status {
        case "active": "Active"
        case "paused": "Paused"
        case "blocked": "Stuck"
        case "usageLimited": "Usage limit hit"
        case "budgetLimited": "Over budget"
        case "complete": "Met"
        case "failed": "Gave up"
        default: ""
        }
    }

    /// The tokens the goal used, against its budget when it has one.
    public static func spent(_ goal: Goal) -> String? {
        switch (goal.tokensUsed ?? 0, goal.tokenBudget ?? 0) {
        case (0, 0): nil
        case (let used, 0): "\(thousands(used)) tokens"
        case (let used, let budget): "\(thousands(used)) / \(thousands(budget)) tokens"
        }
    }
}

/// "16,510".
public func thousands(_ n: Int) -> String {
    let digits = Array(String(n))
    var out = ""
    for (i, c) in digits.enumerated() {
        if i > 0 && (digits.count - i) % 3 == 0 { out.append(",") }
        out.append(c)
    }
    return out
}

/// The model and effort pill.
public enum ModelPill {
    /// Claude Code's model aliases.
    static let claudeAliases = ["haiku", "sonnet", "opus"]

    /// The model's short name and its effort.
    public static func label(_ agent: AgentSummary) -> (model: String, effort: String?) {
        (modelLabel(model: agent.model, provider: agent.provider), agent.effort.flatMap { $0.isEmpty ? nil : effortLabel($0) })
    }

    /// Whether the agent runs `value`; Claude Code reports an alias's full id, as "claude-opus-5-5" for "opus".
    public static func runs(_ agent: AgentSummary, _ value: String) -> Bool {
        guard let m = agent.model else { return false }
        return m == value || agent.provider == "claude" && m.split(separator: "-").contains { $0 == value }
    }

    /// The models a provider offers by alias, the phone having no catalog to list another's.
    public static func aliases(_ provider: String) -> [String] {
        provider == "claude" ? claudeAliases : []
    }

    public static func effortLabel(_ effort: String) -> String {
        switch effort {
        case "": "Agent default"
        case "xhigh": "Extra high"
        default: effort.prefix(1).uppercased() + effort.dropFirst()
        }
    }
}

/// How full the agent's context window is.
public enum ContextMeter {
    public enum Level: Hashable, Sendable {
        case low, warn, danger
    }

    public static let warn = 75
    public static let critical = 90

    /// Tokens used and the window, once the window is known.
    public static func context(_ agent: AgentSummary) -> (used: Int, window: Int)? {
        guard let w = agent.contextWindow, w > 0 else { return nil }
        return (agent.tokensUsed ?? 0, w)
    }

    /// The whole percent of `window` that `used` fills, capped at 100; an empty window reads as 0.
    public static func percent(_ used: Int, _ window: Int) -> Int {
        window > 0 ? min(used * 100 / window, 100) : 0
    }

    /// Judged on the whole percent, so 74.99% is still low at 75.
    public static func level(_ used: Int, _ window: Int, warn: Int = warn, critical: Int = critical) -> Level {
        switch percent(used, window) {
        case let p where p >= critical: .danger
        case let p where p >= warn: .warn
        default: .low
        }
    }

    /// "184,000 / 200,000 tokens" and "16,000 tokens remaining".
    public static func lines(_ used: Int, _ window: Int) -> (used: String, remaining: String) {
        ("\(thousands(used)) / \(thousands(window)) tokens", "\(thousands(max(window - used, 0))) tokens remaining")
    }
}

/// Each chat's draft, while the one input holds a single chat's.
public struct Drafts<T: Sendable>: Sendable {
    /// The agent whose chat the input holds the draft of.
    public private(set) var owner: String?
    var parked: [String: T] = [:]
    let empty: T

    public init(empty: T) { self.empty = empty }

    /// Gives the input to `agent`'s chat; `held` is what the input holds now, for the chat that had it.
    /// Returns `agent`'s draft to put in, or nil when its chat has the input already.
    public mutating func switchTo(_ agent: String, held: () -> T) -> T? {
        if owner == agent { return nil }
        if let owner { parked[owner] = held() }
        owner = agent
        return parked.removeValue(forKey: agent) ?? empty
    }

    /// Forgets a closed chat's draft; true when the input holds it, to empty.
    public mutating func close(_ agent: String) -> Bool {
        parked[agent] = nil
        guard owner == agent else { return false }
        owner = nil
        return true
    }

    /// The draft of a chat that hasn't the input.
    public func parked(_ agent: String) -> T? { parked[agent] }

    /// Every parked draft, to persist.
    public var all: [String: T] { parked }

    public mutating func restore(_ drafts: [String: T]) { parked.merge(drafts) { current, _ in current } }
}
