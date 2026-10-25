import Stores
import Wire

/// What the settings say about a Mac and what this phone may do on it, as plain values.
public enum MacFacts {
    public struct Grant: Hashable, Sendable, Identifiable {
        public var id: String { scope.rawValue }
        public var scope: Scope
        public var title: String
        public var systemImage: String
        public var allowed: Bool
    }

    /// The four phone scopes, each marked as granted or not. A Mac older than scopes.v1 sends none: every one is assumed.
    public nonisolated static func grants(_ scopes: [Scope]?) -> [Grant] {
        let all: [(Scope, String, String)] = [
            (.observe, "See sessions, terminals and changes", "eye"),
            (.drive, "Send prompts and type", "keyboard"),
            (.approve, "Answer permissions", "checkmark.shield"),
            (.spawn, "Start sessions and worktrees", "plus.rectangle.on.rectangle"),
        ]
        return all.map { scope, title, image in
            Grant(scope: scope, title: title, systemImage: image, allowed: scopes?.contains(scope) ?? true)
        }
    }

    /// `phone.maxAccess` as the desktop's Settings → Phone names it.
    public nonisolated static func access(_ value: String) -> String {
        switch value {
        case "ask": "Ask"
        case "edits": "Edits"
        case "auto": "Auto"
        case "full": "Full"
        default: value.capitalized
        }
    }

    /// The Mac's phone.terminals setting; nil from a pocketd too old to say.
    public nonisolated static func terminals(_ on: Bool?) -> String {
        switch on {
        case true?: "On on the Mac"
        case false?: "Off on the Mac"
        case nil: "Set on the Mac"
        }
    }

    /// "Connected · 0.1.13", or how the link reads when it isn't up.
    public nonisolated static func status(_ reachability: Reachability, version: String?) -> String {
        guard reachability == .online, let version else { return reachability.label }
        return "\(reachability.label) · \(version)"
    }
}
