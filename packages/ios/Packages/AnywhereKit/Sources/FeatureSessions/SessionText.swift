import DesignSystem
import Domain
import Stores
import Wire

/// What session rows, the new session sheet and the Macs list say.
enum SessionText {
    static func title(_ s: AgentSummary) -> String {
        s.title.isEmpty ? "New session" : s.title
    }

    /// The line under a session's title: its status and what it is doing, else its model.
    static func detail(_ s: AgentSummary, _ status: Status) -> String? {
        let doing = s.compacting == true ? "Compacting…" : s.activity.flatMap { $0.isEmpty ? nil : $0 }
        let parts = [status.label, doing].compactMap(\.self)
        if !parts.isEmpty { return parts.joined(separator: " · ") }
        return s.model.map(Sidebar.modelName).flatMap { $0.isEmpty ? nil : $0 }
    }

    /// A launch step as pocketd names it.
    static func step(_ step: String) -> String {
        switch step {
        case "prepare": "Preparing"
        case "verify": "Checking the branch"
        case "fetch": "Fetching"
        case "worktree": "Making the worktree"
        case "copy": "Copying files"
        case "setup": "Running setup"
        case "spawn": "Starting the agent"
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
