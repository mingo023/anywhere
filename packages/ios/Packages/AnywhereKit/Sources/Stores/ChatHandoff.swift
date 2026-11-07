import Domain

/// What handing a file, lines or a prompt to a tree's chat does once its `ChatTarget` is read: goes into that
/// session's draft, waits for the reader to pick a session, or starts a new one.
public enum ChatHandoff: Equatable, Sendable {
    case send(ChatSession)
    case choose([ChatSession])
    /// No session takes it: New Session opens on this route, nil when there's no prompt to start one with.
    case start(Route?)

    /// `prompt` is what a new session would start with; a mention has none.
    public init(_ target: ChatTarget, mac: String, project: String?, tree: String, prompt: String?) {
        switch target {
        case .one(let session): self = .send(session)
        case .choose(let sessions): self = .choose(sessions)
        case .none: self = .start(prompt.map { .newSession(mac: mac, project: project, place: tree, prompt: $0) })
        }
    }

    /// The toast once lines are in `session`'s draft.
    public static func added(to session: ChatSession) -> String { "Added to \(session.agent)'s input" }
}
