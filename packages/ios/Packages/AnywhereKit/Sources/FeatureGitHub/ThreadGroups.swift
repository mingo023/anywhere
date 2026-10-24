import Wire

/// How far a review thread has got (`Thread::state` in github.rs). The phone hands nothing to agents, so never Sent.
public enum ThreadState: Hashable, Sendable {
    case open, replied, resolved

    public init(_ t: ReviewThread) {
        if t.resolved {
            self = .resolved
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

    /// The last `n` lines of a comment's hunk, which ends at the line it's on (`hunk_tail`).
    public static func hunkTail(_ hunk: String?, _ n: Int = 3) -> [String] {
        var lines = (hunk ?? "").split(separator: "\n", omittingEmptySubsequences: false)
        if lines.last == "" { lines.removeLast() }
        return lines.filter { !$0.hasPrefix("@@") }.suffix(n).map(String.init)
    }

    /// `path:line`, or the path alone for a thread on the whole file (`place`).
    public static func place(_ t: ReviewThread) -> String {
        t.line.map { "\(t.path):\($0)" } ?? t.path
    }

    /// The reply box's prompt: whom the reply answers.
    public static func replyPrompt(_ t: ReviewThread) -> String {
        "Reply to \(t.comments.first?.author ?? "")…"
    }
}
