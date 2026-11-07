import ChatModel
import Wire

/// A session a PR's problems can be sent to.
struct FixSession: Hashable, Identifiable {
    let id: String
    let title: String
    /// "Claude", "Codex".
    let agent: String
}

/// Which session takes a PR's problems (DET-9): the chat that opened the PR, else the worktree's only session, else
/// the reader's pick of them, most recent first. None on a worktree without a session, which starts one (DET-8).
enum FixTarget: Equatable {
    case one(FixSession)
    case choose([FixSession])
    case none

    /// `opener` is the agent whose chat opened the PR, nil when something else did.
    init(tree: String, agents: [AgentSummary], opener: String?) {
        let sessions = agents.filter { $0.cwd == tree || $0.cwd.hasPrefix(tree + "/") }
            .sorted { $0.updatedAt > $1.updatedAt }
            .map { FixSession(id: $0.id, title: $0.title, agent: who($0.provider)) }
        if let opened = sessions.first(where: { $0.id == opener }) {
            self = .one(opened)
        } else if sessions.count == 1 {
            self = .one(sessions[0])
        } else if sessions.isEmpty {
            self = .none
        } else {
            self = .choose(sessions)
        }
    }

    /// Who the buttons name: the session's agent, the one all the choices share, or "agent"; nil when none.
    var agent: String? {
        switch self {
        case .one(let s): s.agent
        case .choose(let sessions): Set(sessions.map(\.agent)).count == 1 ? sessions[0].agent : "agent"
        case .none: nil
        }
    }

    /// "Send to Claude" for `verb` "Send to"; with no session, starting one.
    func title(_ verb: String) -> String {
        agent.map { "\(verb) \($0)" } ?? "Start a session with this"
    }
}
