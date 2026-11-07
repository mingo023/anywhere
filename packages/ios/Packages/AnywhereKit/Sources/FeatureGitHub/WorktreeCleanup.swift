import Foundation

/// A deletable worktree whose PR is merging or merged, and what deleting it would lose or close.
public struct WorktreeCleanup: Hashable, Sendable {
    public var name: String
    /// Uncommitted files; nil until its status has loaded.
    public var changes: Int?
    /// Sessions running in it.
    public var sessions: Int

    public init(name: String, changes: Int?, sessions: Int) {
        self.name = name
        self.changes = changes
        self.sessions = sessions
    }

    /// The sessions whose folder is in `tree`.
    public static func sessions(in tree: String, cwds: [String]) -> Int {
        cwds.count { $0 == tree || $0.hasPrefix(tree.hasSuffix("/") ? tree : tree + "/") }
    }

    /// Delete after merging starts on only when nothing would be lost or interrupted.
    public var deletesByDefault: Bool { changes == 0 && sessions == 0 }

    /// Deleting asks first unless the worktree is known to be clean: its folder goes with any uncommitted change.
    public var asks: Bool { changes != 0 }

    /// What closes with the worktree, and that the branch stays.
    public var closes: String {
        let what = sessions == 0 ? "Its terminals close" : "Its \(sessions == 1 ? "session" : "\(sessions) sessions") and terminals close"
        return "\(what); the branch stays."
    }

    /// The confirm for a worktree that may hold uncommitted changes.
    public var confirmTitle: String {
        switch changes {
        case let n? where n > 0: "Delete \(name) and its \(n == 1 ? "uncommitted change" : "\(n) uncommitted changes")?"
        default: "Delete \(name)?"
        }
    }

    public var confirmMessage: String {
        changes == nil ? "Anything not committed in it is lost. \(closes)" : "Those changes aren't committed anywhere, so they're lost. \(closes)"
    }
}
