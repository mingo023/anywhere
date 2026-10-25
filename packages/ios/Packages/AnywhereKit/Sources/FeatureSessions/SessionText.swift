import DesignSystem
import Domain
import Stores
import Wire

/// What session rows, the new session sheet and the Macs list say.
enum SessionText {
    static func title(_ s: AgentSummary) -> String {
        s.title.isEmpty ? "New session" : s.title
    }

    /// The line under a session's title: what it is doing, else its status, else its model. Working shows as the
    /// glyph's spin, so it isn't spelled out; the other statuses are, as their dots differ by colour alone.
    static func detail(_ s: AgentSummary, _ status: Status) -> String? {
        if s.compacting == true { return "Compacting…" }
        if let doing = s.activity, !doing.isEmpty { return doing }
        if status != .working, let label = status.label { return label }
        return s.model.map(Sidebar.modelName).flatMap { $0.isEmpty ? nil : $0 }
    }

    /// A launch step as the desktop labels it (`Step::label` in `pocket/src/creating.rs`).
    static func step(_ step: String) -> String {
        switch step {
        case "prepare": "Preparing"
        case "verify": "Checking branch name"
        case "fetch": "Fetching latest changes"
        case "worktree": "Creating git worktree"
        case "copy": "Copying configuration"
        case "setup": "Setting up"
        case "agent": "Starting agent"
        default: step.prefix(1).uppercased() + step.dropFirst()
        }
    }

    static func link(_ link: LinkState) -> (text: String, tone: Pill.Tone) {
        switch link {
        case .connecting: ("Connecting", .neutral)
        case .online: ("Online", .success)
        case .retrying: ("Reconnecting", .waiting)
        case .ended(.revoked): ("Pair again", .failed)
        case .ended(.updateMac): ("Update the Mac", .failed)
        case .ended(.updatePhone): ("Update the app", .failed)
        case .ended: ("Offline", .neutral)
        }
    }

    /// "Work · 42%": an account and the fuller of its two usage windows.
    static func account(_ a: Account) -> String {
        guard let peak = [a.fiveHour?.pct, a.week?.pct].compactMap(\.self).max() else { return a.name }
        return "\(a.name) · \(Int(peak.rounded()))%"
    }

    static func provider(_ id: String) -> String {
        switch id {
        case "claude": "Claude"
        case "codex": "Codex"
        default: id.capitalized
        }
    }

    static func tone(_ status: Status) -> Pill.Tone {
        switch status {
        case .needsYou: .waiting
        case .failed: .failed
        case .done: .success
        case .working: .working
        case .idle: .neutral
        }
    }
}
