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

    public struct Choice: Hashable, Sendable {
        public let value: String
        public let label: String
        public let hint: String
        /// Why a phone can't pick it, as pocketd's `launch.Check` refuses it; nil when it can.
        public let locked: String?
    }

    /// The desktop's choices, a phone's past `max` locked: Full and the agent's own settings, which could allow more,
    /// only from the Mac. A running agent can't switch to its own settings, so `launching` alone offers them.
    public static func choices(max: String, launching: Bool) -> [Choice] {
        let ceiling = text.firstIndex { $0.value == max } ?? 1
        return text.enumerated().compactMap { i, t in
            if t.value == "settings" && !launching { return nil }
            let locked: String? = switch t.value {
            case "settings", "full": "Only from your Mac"
            case _ where i > ceiling: "Raise it on your Mac: ⌘K → Phone access level"
            default: nil
            }
            return Choice(value: t.value, label: t.label, hint: t.hint, locked: locked)
        }
    }
}
