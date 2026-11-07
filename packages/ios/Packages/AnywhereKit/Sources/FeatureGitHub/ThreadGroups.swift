import Stores
import Wire

/// How far a review thread has got (`Thread::state` in github.rs). `sent`: its agent still works on it.
public enum ThreadState: Hashable, Sendable {
    case open, sent, replied, resolved

    public init(_ t: ReviewThread, sent: Bool = false) {
        if t.resolved {
            self = .resolved
        } else if sent {
            self = .sent
        } else if let first = t.comments.first?.author, let last = t.comments.last?.author, first != last {
            self = .replied
        } else {
            self = .open
        }
    }
}

/// Which threads the Comments screen lists.
public struct ThreadFilter: Hashable, Sendable {
    /// Resolved ones too, as the desktop's All segment.
    public var resolved = false
    /// Ones whose line the PR has since changed.
    public var outdated = true

    public init(resolved: Bool = false, outdated: Bool = true) {
        self.resolved = resolved
        self.outdated = outdated
    }

    public func shows(_ t: ReviewThread) -> Bool {
        (resolved || !t.resolved) && (outdated || !t.outdated)
    }
}

/// A file's threads, in the order the PR lists them.
public struct ThreadGroup: Hashable, Sendable, Identifiable {
    public var id: String { path }
    public let path: String
    public let threads: [ReviewThread]
}

public enum ThreadGroups {
    /// The threads `filter` shows, grouped by file in order of each file's first thread.
    public static func group(_ threads: [ReviewThread], _ filter: ThreadFilter) -> [ThreadGroup] {
        var order: [String] = []
        var byPath: [String: [ReviewThread]] = [:]
        for t in threads where filter.shows(t) {
            if byPath[t.path] == nil { order.append(t.path) }
            byPath[t.path, default: []].append(t)
        }
        return order.map { ThreadGroup(path: $0, threads: byPath[$0] ?? []) }
    }

    public static func unresolved(_ threads: [ReviewThread]) -> Int {
        threads.count { !$0.resolved }
    }

    /// What the agent could still be sent: failed checks and open threads it isn't `fixing` (`to_fix`).
    public static func unsent(_ pr: PullRequest, fixing: SentFixes?) -> (checks: [CheckRun], threads: [ReviewThread]) {
        let checks = pr.runs.filter { $0.outcome == "failed" && !(fixing?.checks.contains($0.name) ?? false) }
        let threads = pr.threads.filter { ThreadState($0, sent: fixing?.threads.contains($0.id) ?? false) == .open }
        return (checks, threads)
    }

    /// Why the Comments screen lists nothing.
    public enum Empty: Hashable, Sendable {
        case noComments
        /// Threads the filter hides only for being outdated; Show turns outdated back on.
        case outdatedHidden(Int)
        case allResolved
    }

    /// Why `filter` shows none of `threads`, or nil when it shows some.
    public static func empty(_ threads: [ReviewThread], _ filter: ThreadFilter) -> Empty? {
        guard !threads.contains(where: filter.shows) else { return nil }
        if threads.isEmpty { return .noComments }
        let hidden = threads.count { $0.outdated && (filter.resolved || !$0.resolved) }
        return hidden > 0 ? .outdatedHidden(hidden) : .allResolved
    }

    /// The reply box's prompt: whom the reply answers.
    public static func replyPrompt(_ t: ReviewThread) -> String {
        "Reply to \(t.comments.first?.author ?? "")…"
    }
}
