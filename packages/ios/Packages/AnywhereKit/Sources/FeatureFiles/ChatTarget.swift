import Wire

/// A session the Files screens can hand a file or lines to.
struct ChatSession: Hashable, Identifiable {
    let id: String
    let title: String
    /// The agent's name: "Claude", "Codex".
    let agent: String

    /// The sessions on `tree`, most recent first.
    static func on(_ tree: String, _ agents: [AgentSummary]) -> [ChatSession] {
        agents.filter { $0.cwd == tree || $0.worktree == tree }
            .sorted { $0.updatedAt > $1.updatedAt }
            .map { ChatSession(id: $0.id, title: $0.title, agent: $0.provider.capitalized) }
    }
}

/// Which session takes a mention or a quote (DET-9): the chat that opened Files, else the tree's only session, else
/// the reader's pick of them, most recent first. None on a tree without a session.
enum ChatTarget: Equatable {
    case one(ChatSession)
    case choose([ChatSession])
    case none

    /// `open` holds the chats open on a stack; the one on this tree is the chat that opened Files.
    init(_ sessions: [ChatSession], open: Set<String>) {
        let opened = sessions.filter { open.contains($0.id) }
        if opened.count == 1 {
            self = .one(opened[0])
        } else if sessions.count == 1 {
            self = .one(sessions[0])
        } else if sessions.isEmpty {
            self = .none
        } else {
            self = .choose(sessions)
        }
    }
}
