import Stores
import Testing
import Wire
@testable import FeatureGitHub

private func pr(_ edit: (inout PullRequest) -> Void = { _ in }) -> PullRequest {
    var pr = PullRequest(
        number: 42, title: "Fix flaky checkout test", url: "https://github.com/minh/anywhere/pull/42", state: "OPEN", draft: false, base: "origin/main",
        head: "aaa", checks: Checks(passed: 2, failed: 0, pending: 0), runs: [], reviews: [], requested: [], mergeable: "MERGEABLE", gate: .ready, threads: [])
    edit(&pr)
    return pr
}

@Suite("PR status block")
struct PullRequestSummaryTests {
    @Test("ready to merge says nothing holds the branch")
    func ready() {
        let s = PullRequestText.summary(pr())
        #expect(s.title == "Ready to merge")
        #expect(s.tone == .success)
        #expect(s.detail == "This branch has no conflicts with the base branch.")
    }

    @Test("failed checks count what failed against what passed")
    func failed() {
        let s = PullRequestText.summary(pr { $0.gate = .checksFailed; $0.checks = Checks(passed: 2, failed: 2, pending: 0) })
        #expect(s.title == "Some checks were not successful")
        #expect(s.detail == "2 failing and 2 successful checks")
        #expect(s.tone == .failed)
        #expect(PullRequestText.summary(pr { $0.gate = .checksFailed; $0.checks = Checks(passed: 0, failed: 1, pending: 0) }).detail == "1 failing check")
    }

    @Test("a merge conflict names the base it conflicts with, without origin's prefix")
    func conflicts() {
        let s = PullRequestText.summary(pr { $0.gate = .conflicts })
        #expect(s.title == "This branch has conflicts that must be resolved")
        #expect(s.detail == "Conflicts with main")
    }

    @Test("a merged PR says where it went and a draft says it can't merge")
    func mergedAndDraft() {
        #expect(PullRequestText.summary(pr { $0.state = "MERGED"; $0.gate = .done }).detail == "Merged into main")
        #expect(PullRequestText.summary(pr { $0.draft = true; $0.gate = .draft }).detail == "Drafts can't be merged")
    }
}

@Suite("PR reviews and checks rows")
struct PullRequestRowsTests {
    @Test("the Reviews row takes the PR's verdict, else what its reviews add up to, else who is asked")
    func reviewHeadline() {
        #expect(PullRequestText.reviewHeadline(pr { $0.review = "APPROVED" }).text == "Approved")
        #expect(PullRequestText.reviewHeadline(pr { $0.reviews = [Review(login: "a", state: "APPROVED"), Review(login: "b", state: "CHANGES_REQUESTED")] }).text == "Changes requested")
        #expect(PullRequestText.reviewHeadline(pr { $0.requested = ["Lan Pham"] }) == (.requested, "Review requested"))
        #expect(PullRequestText.reviewHeadline(pr()) == (.none, "No reviews"))
    }

    @Test("reviewers list who reviewed once each, then who is still asked")
    func reviewers() {
        let rows = PullRequestText.reviewers(pr {
            $0.reviews = [Review(login: "rina", state: "CHANGES_REQUESTED"), Review(login: "rina", state: "APPROVED"), Review(login: "bo", state: "COMMENTED")]
            $0.requested = ["bo", "Lan Pham"]
        })
        #expect(rows.map(\.login) == ["rina", "bo", "Lan Pham"])
        #expect(rows.map(\.state) == [.changesRequested, .commented, .pending])
    }

    @Test("the Checks row counts failing before running before passed, and is absent with no checks")
    func checksHeadline() {
        #expect(PullRequestText.checksHeadline(pr { $0.checks = Checks(passed: 4, failed: 2, pending: 1) })?.text == "2 failing")
        #expect(PullRequestText.checksHeadline(pr { $0.checks = Checks(passed: 4, failed: 0, pending: 1) })?.text == "1 running")
        #expect(PullRequestText.checksHeadline(pr { $0.checks = Checks(passed: 4, failed: 0, pending: 0) })?.text == "4 passed")
        #expect(PullRequestText.checksHeadline(pr { $0.checks = Checks(passed: 0, failed: 0, pending: 0) }) == nil)
    }

    @Test("initials take two words' first letters, or a one-word name's first two")
    func initials() {
        #expect(PullRequestText.initials("Lan Pham") == "LP")
        #expect(PullRequestText.initials("fox-api") == "FA")
        #expect(PullRequestText.initials("anywhere") == "AN")
        #expect(PullRequestText.initials("x") == "X")
        #expect(PullRequestText.repoName("https://github.com/minh/anywhere/pull/42") == "anywhere")
    }
}

@Suite("PR header and conversation")
struct PullRequestHeaderTests {
    @Test("the branch sentence sets the branches in mono, with the commits only while open")
    func sentence() {
        let open = PullRequestText.branchSentence(state: "OPEN", base: "origin/main", head: "flaky-checkout", commits: 3)
        #expect(open.map(\.text).joined() == "Merges 3 commits into main from flaky-checkout")
        #expect(open.filter(\.code).map(\.text) == ["main", "flaky-checkout"])
        #expect(PullRequestText.branchSentence(state: "OPEN", base: "main", head: nil, commits: nil).map(\.text).joined() == "Merges into main")
        #expect(PullRequestText.branchSentence(state: "MERGED", base: "main", head: "x", commits: 3).map(\.text).joined() == "Merged into main from x")
    }

    @Test("the conversation lists verdicts, then each thread's first comment oldest first, cut to a line")
    func conversation() {
        let long = String(repeating: "a", count: 120)
        let entries = PullRequestText.conversation(pr {
            $0.reviews = [Review(login: "rina", state: "APPROVED"), Review(login: "bo", state: "COMMENTED"), Review(login: "kai", state: "CHANGES_REQUESTED")]
            $0.threads = [
                ReviewThread(id: "late", path: "a", outdated: false, resolved: false, comments: [ThreadComment(author: "kai", body: "Later\nmore", at: 200)]),
                ReviewThread(id: "early", path: "b", outdated: false, resolved: true, comments: [ThreadComment(author: "bo", body: long, at: 100)]),
            ]
        })
        #expect(entries.map(\.kind) == [.approved, .changesRequested, .comment, .comment])
        #expect(entries.map(\.who) == ["rina", "kai", "bo", "kai"])
        #expect(entries.map(\.what) == ["approved these changes", "requested changes", "commented: " + String(repeating: "a", count: 79) + "…", "commented: Later"])
        #expect(entries.map(\.at) == [nil, nil, 100, 200])
    }
}

@Suite("PR screen on a Mac")
struct PrMacTests {
    @Test("the screen dims while the banner is up and holds still then, and its controls wait whenever the Mac is away")
    func dimming() {
        #expect(!PrMac().dimmed && !PrMac().offline)
        #expect(PrMac(reachability: .reconnecting).dimmed)
        #expect(PrMac(reachability: .unreachable).dimmed)
        #expect(!PrMac(reachability: .connecting).dimmed && PrMac(reachability: .connecting).offline)
        #expect(!PrMac(reachability: .stopped).dimmed && PrMac(reachability: .stopped).writesWait)
        #expect(!PrMac(reachability: .unreachable).writesWait)
    }
}
