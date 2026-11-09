/// The access pill: what the agent may do without asking.
public enum AccessPill {
    static let text: [(value: String, label: String, hint: String)] = [
        ("settings", "Agent's settings", "Use the agent's own permission config."),
        ("ask", "Ask", "Ask before commands and file changes."),
        ("edits", "Edits", "Auto-approve edits, ask before other actions."),
        ("auto", "Auto", "A reviewer model approves or denies actions."),
        ("full", "Full", "Skip every approval."),
    ]

    public static func label(_ access: String) -> String {
        (text.first { $0.value == access } ?? text[0]).label
    }

    /// Full skips every approval, so it stands out.
    public static func warns(_ access: String) -> Bool { access == "full" }

    /// Whether switching from `current` to `access` asks first: a phone is a tap away from skipping every approval.
    public static func confirms(_ access: String, from current: String) -> Bool { warns(access) && !warns(current) }

    public struct Choice: Hashable, Sendable {
        public let value: String
        public let label: String
        public let hint: String
    }

    /// The desktop's choices. A running agent can't switch to its own settings, so `launching` alone offers them.
    public static func choices(launching: Bool) -> [Choice] {
        text.compactMap { t in
            t.value == "settings" && !launching ? nil : Choice(value: t.value, label: t.label, hint: t.hint)
        }
    }
}
