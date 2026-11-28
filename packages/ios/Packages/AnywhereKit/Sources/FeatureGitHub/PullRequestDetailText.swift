import DesignSystem
import Foundation
import Stores
import Wire

/// What the PR screen's Status block says, in its colour: the footer under the reviews and checks.
public struct PrSummary: Equatable, Sendable {
    public var tone: PrTone
    public var icon: Lucide
    public var title: String
    public var detail: String
}

/// A reviewer's place in the Reviews row.
public struct Reviewer: Equatable, Sendable, Identifiable {
    public enum State: Equatable, Sendable {
        case approved, changesRequested, commented, pending

        public var label: String {
            switch self {
            case .approved: "Approved"
            case .changesRequested: "Changes requested"
            case .commented: "Commented"
            case .pending: "Pending"
            }
        }
    }

    public var login: String
    public var state: State
    public var id: String { login }
}

/// A line of the Conversation: who did what, and when when the wire says.
public struct ConversationEntry: Equatable, Sendable, Identifiable {
    public enum Kind: Equatable, Sendable {
        case approved, changesRequested, comment
    }

    public var id: String
    public var kind: Kind
    public var who: String
    public var what: String
    /// Unix seconds.
    public var at: Int?
}

/// A run of the branch sentence, set in mono when it names a branch.
public struct SentencePiece: Equatable, Sendable {
    public var text: String
    public var code = false
}

extension PullRequestText {
    /// Where the merge stands, as the Status block's footer words it.
    public static func summary(_ pr: PullRequest) -> PrSummary {
        switch pr.state {
        case "MERGED": return PrSummary(tone: .merged, icon: .merge, title: "Pull request merged", detail: "Merged into \(baseBranch(pr.base))")
        case "CLOSED": return PrSummary(tone: .quiet, icon: .pullRequestClosed, title: "Pull request closed", detail: "Closed without merging")
        default: break
        }
        let c = pr.checks
        let base = baseBranch(pr.base)
        switch pr.gate {
        case .ready:
            return PrSummary(tone: .success, icon: .circleCheck, title: "Ready to merge", detail: "This branch has no conflicts with the base branch.")
        case .checksFailed:
            let failing = "\(c.failed) failing"
            let detail = c.passed > 0 ? "\(failing) and \(c.passed) successful checks" : "\(failing) \(c.failed == 1 ? "check" : "checks")"
            return PrSummary(tone: .failed, icon: .circleX, title: "Some checks were not successful", detail: detail)
        case .conflicts:
            return PrSummary(tone: .failed, icon: .circleAlert, title: "This branch has conflicts that must be resolved", detail: gate(.conflicts, base: base))
        case .checksRunning:
            return PrSummary(tone: .waiting, icon: .clock, title: "Checks are in progress", detail: "\(plural(c.pending, "check")) still running")
        case .changesRequested:
            return PrSummary(tone: .failed, icon: .circleAlert, title: "Changes requested", detail: gate(.changesRequested, base: base))
        case .reviewRequired:
            return PrSummary(tone: .waiting, icon: .circleAlert, title: "Review required", detail: gate(.reviewRequired, base: base))
        case .blocked:
            return PrSummary(tone: .waiting, icon: .circleAlert, title: "Merging is blocked", detail: gate(.blocked, base: base))
        case .draft:
            return PrSummary(tone: .quiet, icon: .pullRequestDraft, title: "This pull request is a draft", detail: gate(.draft, base: base))
        default:
            return PrSummary(tone: .quiet, icon: .clock, title: "Checking for conflicts…", detail: "")
        }
    }

    /// The Reviews row's verdict: the PR's own, else what its reviews add up to, else who is asked.
    public static func reviewHeadline(_ pr: PullRequest) -> (mark: ReviewMark, text: String) {
        let verdict = pr.review ?? (pr.reviews.contains { $0.state == "CHANGES_REQUESTED" } ? "CHANGES_REQUESTED"
            : pr.reviews.contains { $0.state == "APPROVED" } ? "APPROVED" : nil)
        switch verdict {
        case "APPROVED": return (.approved, "Approved")
        case "CHANGES_REQUESTED": return (.changesRequested, "Changes requested")
        default: return pr.requested.isEmpty ? (.none, "No reviews") : (.requested, "Review requested")
        }
    }

    /// Who reviewed, in the order they did, then who is still asked.
    public static func reviewers(_ pr: PullRequest) -> [Reviewer] {
        var seen: Set<String> = []
        var rows: [Reviewer] = []
        for r in pr.reviews where !seen.contains(r.login) {
            seen.insert(r.login)
            let state: Reviewer.State = switch r.state {
            case "APPROVED": .approved
            case "CHANGES_REQUESTED": .changesRequested
            default: .commented
            }
            rows.append(Reviewer(login: r.login, state: state))
        }
        for login in pr.requested where !seen.contains(login) {
            seen.insert(login)
            rows.append(Reviewer(login: login, state: .pending))
        }
        return rows
    }

    /// The Checks row's tally: "2 failing", "1 running", "4 passed"; nil with no checks.
    public static func checksHeadline(_ pr: PullRequest) -> (tone: PrTone, text: String)? {
        let c = pr.checks
        if c.failed > 0 { return (.failed, "\(c.failed) failing") }
        if c.pending > 0 { return (.waiting, "\(c.pending) running") }
        if c.passed > 0 { return (.success, "\(c.passed) passed") }
        return nil
    }

    /// "Lan Pham" is "LP", "fox-api" is "FA", "anywhere" is "AN".
    public static func initials(_ name: String) -> String {
        let words = name.split { " -_.".contains($0) }
        if words.count > 1 { return words.prefix(2).compactMap(\.first).map { String($0).uppercased() }.joined() }
        return String(name.prefix(2)).uppercased()
    }

    /// "anywhere" from a PR's URL.
    public static func repoName(_ url: String) -> String? {
        repo(url)?.split(separator: "/").last.map(String.init)
    }

    /// Where the branch goes: "Merges 3 commits into main from flaky-checkout". The wire names no author, so no one wants it.
    public static func branchSentence(state: String, base: String, head: String?, commits: Int?) -> [SentencePiece] {
        let base = baseBranch(base)
        var pieces: [SentencePiece]
        switch state {
        case "MERGED": pieces = [SentencePiece(text: "Merged into "), SentencePiece(text: base, code: true)]
        case "CLOSED": pieces = [SentencePiece(text: "Closed, was going into "), SentencePiece(text: base, code: true)]
        default:
            let count = commits.map { " \(Self.commits($0))" } ?? ""
            pieces = [SentencePiece(text: "Merges\(count) into "), SentencePiece(text: base, code: true)]
        }
        if let head { pieces += [SentencePiece(text: " from "), SentencePiece(text: head, code: true)] }
        return pieces
    }

    /// The reviews' verdicts, then each thread's opening comment oldest first. Only comments carry a time.
    public static func conversation(_ pr: PullRequest) -> [ConversationEntry] {
        var entries: [ConversationEntry] = pr.reviews.compactMap { r in
            switch r.state {
            case "APPROVED": ConversationEntry(id: "review-\(r.login)", kind: .approved, who: r.login, what: "approved these changes")
            case "CHANGES_REQUESTED": ConversationEntry(id: "review-\(r.login)", kind: .changesRequested, who: r.login, what: "requested changes")
            default: nil
            }
        }
        let comments = pr.threads.compactMap { t in
            t.comments.first.map { ConversationEntry(id: "thread-\(t.id)", kind: .comment, who: $0.author, what: "commented: \(oneLine($0.body))", at: $0.at) }
        }
        entries += comments.sorted { ($0.at ?? 0) < ($1.at ?? 0) }
        return entries
    }

    /// The first line of a comment, cut at 80 characters.
    static func oneLine(_ body: String) -> String {
        let line = body.split(whereSeparator: \.isNewline).first.map { String($0).trimmingCharacters(in: .whitespaces) } ?? ""
        return line.count > 80 ? String(line.prefix(79)) + "…" : line
    }
}
