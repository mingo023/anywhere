import Wire

/// The colour a PR's words take, as the desktop's theme tokens name them.
public enum PrTone: Hashable, Sendable {
    case success, failed, waiting, merged, muted, quiet
}

/// What the PR card says, ported from the desktop's `pull_request.rs` and `pull_requests.rs` so both word it alike.
public enum PullRequestText {
    /// What holds the merge back, or that nothing does (`gate_text`).
    public static func gate(_ gate: Gate, base: String) -> String {
        switch gate {
        case .ready: "No conflicts with \(base)"
        case .conflicts: "Conflicts with \(base)"
        case .checking: "Checking for conflicts…"
        case .checksRunning: "Checks in progress"
        case .checksFailed: "Can't merge until checks pass"
        case .changesRequested: "Can't merge until the requested changes are approved"
        case .reviewRequired: "Needs an approving review"
        case .blocked: "A branch rule holds the merge"
        case .draft: "Drafts can't be merged"
        default: ""
        }
    }

    /// Whether it merges cleanly into its base, when the gate hasn't said so already.
    public static func mergeable(_ pr: PullRequest) -> String? {
        if [.ready, .conflicts, .checking, .done].contains(pr.gate) { return nil }
        switch pr.mergeable {
        case "MERGEABLE": return gate(.ready, base: pr.base)
        case "CONFLICTING": return gate(.conflicts, base: pr.base)
        default: return gate(.checking, base: pr.base)
        }
    }

    /// Where the PR stands in one line, in its colour: all a folded card says (`verdict`). `busy` is what gh is doing for it.
    public static func verdict(_ pr: PullRequest, busy: String? = nil) -> (PrTone, String) {
        switch pr.state {
        case "MERGED": return (.merged, "Merged into \(pr.base)")
        case "CLOSED": return (.muted, "Closed")
        default: break
        }
        if let busy { return (.waiting, busy) }
        let c = pr.checks
        switch pr.gate {
        case .draft: return (.muted, "Draft")
        case .ready: return (.success, "Ready to merge")
        case .conflicts: return (.failed, gate(.conflicts, base: pr.base))
        case .checksFailed where pr.review == "CHANGES_REQUESTED": return (.failed, "\(plural(c.failed, "check")) failed · changes requested")
        case .checksFailed: return (.failed, "\(plural(c.failed, "check")) failed")
        case .checksRunning: return (.waiting, "\(plural(c.pending, "check")) in progress")
        case .changesRequested: return (.failed, "Changes requested")
        default: return (.quiet, gate(pr.gate, base: pr.base))
        }
    }

    /// A list row's colour: by state, then by its worst check (`tone`).
    public static func tone(state: String, draft: Bool, checks c: Checks) -> PrTone {
        switch state {
        case "MERGED": .merged
        case "CLOSED": .muted
        case _ where draft: .muted
        case _ where c.failed > 0: .failed
        case _ where c.pending > 0: .waiting
        case _ where c.passed > 0: .success
        default: .quiet
        }
    }

    /// "Draft · 1 pending", "3 failed · 2 pending · 3 passed", "Merged · No checks" (`detail`).
    public static func detail(state: String, draft: Bool, checks c: Checks) -> String {
        let label: String? = switch state {
        case "MERGED": "Merged"
        case "CLOSED": "Closed"
        default: draft ? "Draft" : nil
        }
        let counts = tally([(c.failed, "failed"), (c.pending, "pending"), (c.passed, "passed")])
        let checks = counts.isEmpty ? "No checks" : counts
        return label.map { "\($0) · \(checks)" } ?? checks
    }

    /// The PR's icon and colour, as GitHub shows its state whatever its checks (`glyph`).
    public static func glyph(state: String, draft: Bool) -> (systemImage: String, tone: PrTone) {
        switch state {
        case "MERGED": ("arrow.triangle.merge", .merged)
        case "CLOSED": ("xmark.circle", .failed)
        default: draft ? ("circle.dashed", .muted) : ("arrow.triangle.pull", .success)
        }
    }

    /// "5 passed · 1 skipped", leaving out what is none.
    public static func tally(_ parts: [(Int, String)]) -> String {
        parts.filter { $0.0 > 0 }.map { "\($0.0) \($0.1)" }.joined(separator: " · ")
    }

    public static func plural(_ n: Int, _ one: String) -> String {
        n == 1 ? "1 \(one)" : "\(n) \(one)s"
    }

    /// A reviewer's verdict or a pending ask, for the review rows.
    public enum ReviewMark: Hashable, Sendable {
        case approved, changesRequested, requested, none
    }

    /// Who approved or asked for changes, then who is still asked; "No review yet" when nobody (`review_lines`).
    public static func reviews(_ pr: PullRequest) -> [(ReviewMark, String)] {
        var lines: [(ReviewMark, String)] = pr.reviews.compactMap { r in
            switch r.state {
            case "APPROVED": (.approved, "\(r.login) approved")
            case "CHANGES_REQUESTED": (.changesRequested, "\(r.login) requested changes")
            default: nil
            }
        }
        if !pr.requested.isEmpty { lines.append((.requested, "Review requested from \(pr.requested.joined(separator: ", "))")) }
        if lines.isEmpty { lines.append((.none, "No review yet")) }
        return lines
    }

    /// The checks named one by one, failed before pending; the passed and skipped ones are only counted (`check_rows`).
    public struct CheckRows: Equatable, Sendable {
        public var named: [CheckRun]
        public var passed: Int
        public var skipped: Int
    }

    public static func checkRows(_ pr: PullRequest) -> CheckRows {
        let failed = pr.runs.filter { $0.outcome == "failed" }
        let pending = pr.runs.filter { $0.outcome == "pending" }
        return CheckRows(
            named: failed + pending, passed: pr.runs.filter { $0.outcome == "passed" }.count,
            skipped: pr.runs.filter { $0.outcome == "skipped" }.count)
    }

    /// One line over the checks: "1 failed · 1 pending · 5 passed", or that there are none.
    public static func checksSummary(_ pr: PullRequest) -> String {
        let rows = checkRows(pr)
        let s = tally([(pr.checks.failed, "failed"), (pr.checks.pending, "pending"), (rows.passed, "passed"), (rows.skipped, "skipped")])
        return s.isEmpty ? "No checks on this branch" : s
    }

    /// The SF Symbol and colour of a check's outcome.
    public static func outcome(_ outcome: String) -> (systemImage: String, tone: PrTone) {
        switch outcome {
        case "failed": ("xmark.circle.fill", .failed)
        case "pending": ("clock.fill", .waiting)
        case "skipped": ("arrow.uturn.right.circle", .muted)
        default: ("checkmark.circle.fill", .success)
        }
    }

    /// The create button's words (`composer.rs`).
    public static func createLabel(draft: Bool, busy: String?) -> String {
        busy ?? (draft ? "Create draft PR" : "Create PR")
    }

    /// A title and body from the branch's commits, given newest first, for when no agent writes them (`from_commits`).
    public static func fromCommits(_ commits: [CommitRef]) -> (title: String, body: String) {
        let title = commits.last?.subject ?? ""
        let body = commits.count > 1 ? commits.reversed().map { "- \($0.subject)" }.joined(separator: "\n") : ""
        return (title, body)
    }
}
