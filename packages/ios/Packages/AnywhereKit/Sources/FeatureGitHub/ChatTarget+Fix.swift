import Stores

extension ChatTarget {
    /// Who the PR screens' buttons name: the session's agent, the one all the choices share, or "agent"; nil when none.
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
