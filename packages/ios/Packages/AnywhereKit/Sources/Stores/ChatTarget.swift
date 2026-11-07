import Domain
import Wire

/// A session the Files, diff and PR screens can hand a file, lines or a PR's problems to.
public struct ChatSession: Hashable, Identifiable, Sendable {
    public let id: String
    public let title: String
    /// The agent's name: "Claude", "Codex".
    public let agent: String
    /// It can take a prompt: it isn't waiting on a permission or a question.
    public let ready: Bool

    public init(id: String, title: String, agent: String, ready: Bool = true) {
        self.id = id
        self.title = title
        self.agent = agent
        self.ready = ready
    }

    /// The sessions on `tree`, most recent first: those whose folder it holds more closely than any of `trees`, the Mac's
    /// worktrees, so a worktree inside the main tree keeps its own.
    public static func on(_ tree: String, trees: [String], _ agents: [AgentSummary]) -> [ChatSession] {
        agents.filter { Sidebar.worktreeOf($0.cwd, trees: trees + [tree]) == tree }
            .sorted { $0.updatedAt > $1.updatedAt }
            .map { ChatSession(id: $0.id, title: $0.title, agent: $0.provider.capitalized, ready: $0.status != .needsYou) }
    }
}

/// Which session takes a mention or a quote (DET-9): the chat that opened the screen, else the tree's only session, else
/// the reader's pick of them, most recent first, counting only those that can take a prompt. None on a tree without a session.
public enum ChatTarget: Equatable, Sendable {
    case one(ChatSession)
    case choose([ChatSession])
    case none

    /// `opener` is the agent whose chat opened the screen, nil when something else did.
    public init(_ sessions: [ChatSession], opener: String?) {
        // With none ready, a waiting one still takes it and says why, rather than a second session starting.
        let ready = sessions.filter(\.ready)
        let sessions = ready.isEmpty ? sessions : ready
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
}
