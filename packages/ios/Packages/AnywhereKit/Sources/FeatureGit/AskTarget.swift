/// Which session a quote from a diff goes to: the worktree's only session, else one the user picks from its sessions,
/// newest first. With none, Ask in Chat isn't offered.
enum AskTarget: Equatable {
    case none
    case one(String)
    case pick([String])

    /// `sessions` newest first, each with the folder its agent runs in.
    static func of(tree: String, sessions: [(id: String, cwd: String)]) -> AskTarget {
        let here = sessions.filter { $0.cwd == tree || $0.cwd.hasPrefix(tree + "/") }.map(\.id)
        switch here.count {
        case 0: return .none
        case 1: return .one(here[0])
        default: return .pick(here)
        }
    }

    /// A draft with `quote` added below what's typed.
    static func appending(_ quote: String, to draft: String) -> String {
        if draft.allSatisfy(\.isWhitespace) { return quote + "\n" }
        return draft + (draft.hasSuffix("\n") ? "\n" : "\n\n") + quote + "\n"
    }
}
