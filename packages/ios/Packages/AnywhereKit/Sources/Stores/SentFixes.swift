import Wire

/// What a tree's agent was sent to fix on its PR since the PR's head last moved (`Sent` in the desktop's pull_request.rs).
public struct SentFixes: Hashable, Sendable {
    public var head: String
    public var threads: Set<String> = []
    /// Check names.
    public var checks: Set<String> = []
    public var conflict = false

    public init(head: String) { self.head = head }

    public var items: Int { threads.count + checks.count }

    /// `sent` with these added for `pr`; what was sent before its head moved is done.
    public static func adding(to sent: SentFixes?, _ pr: PullRequest, threads: [String], checks: [String], conflict: Bool) -> SentFixes {
        var s = sent.flatMap { $0.head == pr.head ? $0 : nil } ?? SentFixes(head: pr.head)
        s.threads.formUnion(threads)
        s.checks.formUnion(checks)
        s.conflict = s.conflict || conflict
        return s
    }

    /// What the agent still works on, less what `pr` already shows done, as a fix that needed no push leaves the head
    /// where it was (`fixing`). Nil once nothing is left or the head moved.
    public func left(in pr: PullRequest) -> SentFixes? {
        guard head == pr.head else { return nil }
        var left = SentFixes(head: head)
        left.threads = threads.filter { id in !pr.threads.contains { $0.id == id && $0.resolved } }
        left.checks = checks.filter { name in
            let runs = pr.runs.filter { $0.name == name }
            return runs.isEmpty || !runs.allSatisfy { $0.outcome == "passed" || $0.outcome == "skipped" }
        }
        left.conflict = conflict && pr.mergeable != "MERGEABLE"
        return left.items > 0 || left.conflict ? left : nil
    }
}
