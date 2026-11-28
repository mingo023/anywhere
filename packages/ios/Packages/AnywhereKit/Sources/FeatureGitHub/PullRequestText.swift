import DesignSystem
import Foundation
import Wire

/// The colour a PR's words take, as the desktop's theme tokens name them.
public enum PrTone: Hashable, Sendable {
    case success, failed, waiting, merged, muted, quiet
}

/// What the PR card says, ported from the desktop's `pull_request.rs` and `pull_requests.rs` so both word it alike.
public enum PullRequestText {
    /// A base as gh names it: origin's copy by its branch name (`base_branch`).
    public static func baseBranch(_ base: String) -> String {
        base.hasPrefix("origin/") ? String(base.dropFirst("origin/".count)) : base
    }

    /// Create PR is offered only for a branch off its base, as the desktop's Changes menu does.
    public static func offersCreate(branch: String?, base: String?) -> Bool {
        guard let branch, let base else { return false }
        return baseBranch(base) != branch
    }

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

    /// The PR's icon and colour, as GitHub shows its state whatever its checks (`glyph`).
    public static func glyph(state: String, draft: Bool) -> (icon: Lucide, tone: PrTone) {
        switch state {
        case "MERGED": (.merge, .merged)
        case "CLOSED": (.pullRequestClosed, .failed)
        default: draft ? (.pullRequestDraft, .quiet) : (.pullRequest, .success)
        }
    }

    /// The state GitHub's badge names: Open, Draft, Merged or Closed.
    public static func stateLabel(state: String, draft: Bool) -> String {
        switch state {
        case "MERGED": "Merged"
        case "CLOSED": "Closed"
        default: draft ? "Draft" : "Open"
        }
    }

    /// "5 passed · 1 skipped", leaving out what is none.
    public static func tally(_ parts: [(Int, String)]) -> String {
        parts.filter { $0.0 > 0 }.map { "\($0.0) \($0.1)" }.joined(separator: " · ")
    }

    /// While the agent sent the conflict works on it (`resolving_text`).
    public static func resolving(_ agent: String) -> String {
        "\(agent) is resolving the conflict"
    }

    public static func plural(_ n: Int, _ one: String) -> String {
        n == 1 ? "1 \(one)" : "\(n) \(one)s"
    }

    /// A reviewer's verdict or a pending ask, for the review rows.
    public enum ReviewMark: Hashable, Sendable {
        case approved, changesRequested, requested, none
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

    /// "1 commit", "3 commits" (`commits`).
    public static func commits(_ n: Int) -> String {
        n == 1 ? "1 commit" : "\(n) commits"
    }

    /// "fix-cap → main · 3 commits ahead": what a new PR would open, as the composer's top line.
    public static func headLine(branch: String, base: String, ahead: Int?) -> String {
        let line = "\(branch) → \(baseBranch(base))"
        return ahead.map { "\(line) · \(commits($0)) ahead" } ?? line
    }

    /// "acme/app" from a PR's URL, the repository a merge method is remembered for.
    public static func repo(_ url: String) -> String? {
        guard let path = URL(string: url)?.path.split(separator: "/"), path.count >= 2 else { return nil }
        return "\(path[0])/\(path[1])"
    }

    /// Whose name a review reply posts under.
    public static func postsAs(login: String?, mac: String) -> String {
        login.map { "Posts to GitHub as @\($0) from \(mac)" } ?? "Posts to GitHub as \(mac)'s gh account"
    }

    /// The create button's words (`composer.rs`).
    public static func createLabel(draft: Bool, busy: String?) -> String {
        busy ?? (draft ? "Create draft" : "Create")
    }

    /// A title and body from the branch's commits, given newest first, for when no agent writes them (`from_commits`).
    public static func fromCommits(_ commits: [CommitRef]) -> (title: String, body: String) {
        let title = commits.last?.subject ?? ""
        let body = commits.count > 1 ? commits.reversed().map { "- \($0.subject)" }.joined(separator: "\n") : ""
        return (title, body)
    }
}
