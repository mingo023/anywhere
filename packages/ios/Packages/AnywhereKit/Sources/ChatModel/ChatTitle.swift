import Domain
import Wire

/// The chat's bar: the session's name over where its agent stands.
public enum ChatTitle {
    /// "Needs you" or "Failed" while the agent wants a look, "Working" while it works or the chat is `busy` sending,
    /// else the Mac it runs on.
    public static func subtitle(_ agent: AgentSummary?, busy: Bool, mac: String) -> String {
        guard let agent else { return mac }
        let status = Status(status: agent.status.rawValue, failed: agent.failed ?? false, attached: agent.attached)
        switch status {
        case .needsYou, .failed, .working: return status.label ?? mac
        default: return busy ? "Working" : mac
        }
    }

    public static func branch(_ branch: String?, detached: Bool) -> String {
        detached ? "Detached HEAD" : branch ?? "No branch"
    }

    /// "+12 −3", or "2 files" for changes git counts no lines of, such as new files; nil on a clean tree.
    public static func change(files: Int, added: Int, removed: Int) -> String? {
        guard files > 0 else { return nil }
        if added + removed > 0 { return "+\(added) −\(removed)" }
        return files == 1 ? "1 file" : "\(files) files"
    }
}
