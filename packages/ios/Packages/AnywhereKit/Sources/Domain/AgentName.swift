/// The agents this build knows: what each is called and what it calls its effort.
public enum AgentName: String, CaseIterable, Sendable {
    case claude, codex, cursor, grok, opencode, pi, omp, fx, hermes, antigravity

    struct Text {
        let full: String
        let short: String
        let effort: String
    }

    var text: Text {
        switch self {
        case .claude: Text(full: "Claude Code", short: "Claude", effort: "Effort")
        case .codex: Text(full: "Codex", short: "Codex", effort: "Reasoning")
        case .cursor: Text(full: "Cursor", short: "Cursor", effort: "Effort")
        case .grok: Text(full: "Grok Build", short: "Grok", effort: "Reasoning")
        case .opencode: Text(full: "OpenCode", short: "OpenCode", effort: "Variant")
        case .pi: Text(full: "Pi", short: "Pi", effort: "Thinking")
        case .omp: Text(full: "omp", short: "omp", effort: "Thinking")
        case .fx: Text(full: "fx", short: "fx", effort: "Effort")
        case .hermes: Text(full: "Hermes Agent", short: "Hermes", effort: "Effort")
        case .antigravity: Text(full: "Antigravity", short: "Antigravity", effort: "Effort")
        }
    }

    /// Its name in full, as pocketd's alerts title a session that has none; nil for an agent this build doesn't know.
    public static func full(_ provider: String) -> String? { AgentName(rawValue: provider)?.text.full }

    /// The name a row has room for: "Claude", "Hermes".
    public static func short(_ provider: String) -> String? { AgentName(rawValue: provider)?.text.short }

    /// `short`, or its id with a capital for an agent this build doesn't know.
    public static func label(_ provider: String) -> String { short(provider) ?? provider.prefix(1).uppercased() + provider.dropFirst() }

    /// What the agent calls effort, as its menu row reads.
    public static func effort(_ provider: String) -> String { AgentName(rawValue: provider)?.text.effort ?? "Effort" }
}
