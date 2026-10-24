import Fixtures
import Foundation
import Testing
import Wire
@testable import FeatureGitHub

private func pr(_ edit: (inout PullRequest) -> Void = { _ in }) -> PullRequest {
    var pr = PullRequest(
        number: 12, title: "t", url: "https://github.com/acme/app/pull/12", state: "OPEN", draft: false, base: "main", head: "aaa",
        checks: Checks(passed: 0, failed: 0, pending: 0), runs: [], reviews: [], requested: [], mergeable: "MERGEABLE", gate: .ready, threads: [])
    edit(&pr)
    return pr
}

private func check(_ name: String, _ outcome: String) -> CheckRun { CheckRun(name: name, outcome: outcome) }

private func thread(_ id: String, path: String = "src/a.rs", resolved: Bool = false, outdated: Bool = false, authors: [String] = ["mira", "bo"]) -> ReviewThread {
    ReviewThread(id: id, path: path, line: 42, outdated: outdated, resolved: resolved,
                 comments: authors.map { ThreadComment(author: $0, body: "said", at: 0, hunk: "@@ -1,4 +1,4 @@\n a\n b\n-c\n+d\n e") })
}

@Suite("PR card wording")
struct PullRequestTextTests {
    @Test("the merge row says what holds it back")
    func gateText() {
        #expect(PullRequestText.gate(.ready, base: "main") == "No conflicts with main")
        #expect(PullRequestText.gate(.conflicts, base: "main") == "Conflicts with main")
        #expect(PullRequestText.gate(.checksRunning, base: "main") == "Checks in progress")
        #expect(PullRequestText.gate(.checking, base: "main") == "Checking for conflicts…")
        #expect(PullRequestText.gate(.reviewRequired, base: "main") == "Needs an approving review")
        #expect(PullRequestText.gate(.blocked, base: "main") == "A branch rule holds the merge")
        #expect(PullRequestText.gate(.draft, base: "main") == "Drafts can't be merged")
        #expect(PullRequestText.gate(.done, base: "main") == "")
    }

    @Test("the merge section says whether it merges cleanly unless the gate already did")
    func mergeable() {
        #expect(PullRequestText.mergeable(pr { $0.gate = .checksFailed }) == "No conflicts with main")
        #expect(PullRequestText.mergeable(pr { $0.gate = .draft; $0.mergeable = "UNKNOWN" }) == "Checking for conflicts…")
        #expect(PullRequestText.mergeable(pr { $0.gate = .ready }) == nil)
        #expect(PullRequestText.mergeable(pr { $0.gate = .conflicts; $0.mergeable = "CONFLICTING" }) == nil)
    }

    @Test("a folded card says what holds the PR back in its colour")
    func verdict() {
        let failed = Checks(passed: 5, failed: 2, pending: 1)
        #expect(PullRequestText.verdict(pr { $0.checks = failed; $0.review = "CHANGES_REQUESTED"; $0.gate = .checksFailed }) == (.failed, "2 checks failed · changes requested"))
        #expect(PullRequestText.verdict(pr { $0.checks = Checks(passed: 0, failed: 0, pending: 1); $0.gate = .checksRunning }) == (.waiting, "1 check in progress"))
        #expect(PullRequestText.verdict(pr { $0.checks = Checks(passed: 3, failed: 0, pending: 0); $0.review = "APPROVED" }) == (.success, "Ready to merge"))
        #expect(PullRequestText.verdict(pr { $0.mergeable = "CONFLICTING"; $0.gate = .conflicts }) == (.failed, "Conflicts with main"))
        #expect(PullRequestText.verdict(pr { $0.draft = true; $0.gate = .draft }) == (.muted, "Draft"))
        #expect(PullRequestText.verdict(pr { $0.gate = .reviewRequired }) == (.quiet, "Needs an approving review"))
    }

    @Test("what gh is doing comes before what holds the PR back")
    func busy() {
        #expect(PullRequestText.verdict(pr { $0.gate = .checksFailed }, busy: "Merging…") == (.waiting, "Merging…"))
    }

    @Test("a merged card names where it went")
    func merged() {
        #expect(PullRequestText.verdict(pr { $0.state = "MERGED"; $0.gate = .done }, busy: "Merging…") == (.merged, "Merged into main"))
        #expect(PullRequestText.verdict(pr { $0.state = "CLOSED"; $0.gate = .done }) == (.muted, "Closed"))
    }

    @Test("the review lists verdicts then who is still asked")
    func reviews() {
        let p = pr {
            $0.reviews = [Review(login: "mira", state: "APPROVED"), Review(login: "bo", state: "COMMENTED"), Review(login: "li", state: "CHANGES_REQUESTED")]
            $0.requested = ["kai", "jo"]
        }
        #expect(PullRequestText.reviews(p).map(\.1) == ["mira approved", "li requested changes", "Review requested from kai, jo"])
        #expect(PullRequestText.reviews(pr()).map(\.0) == [.none])
        #expect(PullRequestText.reviews(pr()).map(\.1) == ["No review yet"])
    }

    @Test("failed checks are named before running ones and the rest are only counted")
    func checkRows() {
        let rows = PullRequestText.checkRows(pr { $0.runs = [check("a", "passed"), check("b", "pending"), check("c", "failed"), check("d", "skipped"), check("e", "passed")] })
        #expect(rows.named.map(\.name) == ["c", "b"])
        #expect(rows.passed == 2 && rows.skipped == 1)
    }

    @Test("counted checks leave out what is none")
    func tally() {
        #expect(PullRequestText.tally([(5, "passed"), (1, "skipped")]) == "5 passed · 1 skipped")
        #expect(PullRequestText.tally([(3, "passed"), (0, "skipped")]) == "3 passed")
        #expect(PullRequestText.tally([(0, "passed"), (2, "skipped")]) == "2 skipped")
    }

    @Test("a list row names the state and counts the checks")
    func detail() {
        #expect(PullRequestText.detail(state: "OPEN", draft: false, checks: Checks(passed: 3, failed: 3, pending: 2)) == "3 failed · 2 pending · 3 passed")
        #expect(PullRequestText.detail(state: "MERGED", draft: false, checks: Checks(passed: 0, failed: 0, pending: 0)) == "Merged · No checks")
        #expect(PullRequestText.detail(state: "OPEN", draft: true, checks: Checks(passed: 0, failed: 0, pending: 1)) == "Draft · 1 pending")
    }

    @Test("a row is toned by state then by its worst check")
    func tone() {
        let c = { (f: Int, p: Int, ok: Int) in Checks(passed: ok, failed: f, pending: p) }
        let tones = [
            PullRequestText.tone(state: "MERGED", draft: false, checks: c(1, 0, 0)),
            PullRequestText.tone(state: "CLOSED", draft: false, checks: c(0, 0, 1)),
            PullRequestText.tone(state: "OPEN", draft: true, checks: c(1, 0, 0)),
            PullRequestText.tone(state: "OPEN", draft: false, checks: c(1, 1, 1)),
            PullRequestText.tone(state: "OPEN", draft: false, checks: c(0, 1, 1)),
            PullRequestText.tone(state: "OPEN", draft: false, checks: c(0, 0, 1)),
            PullRequestText.tone(state: "OPEN", draft: false, checks: c(0, 0, 0)),
        ]
        #expect(tones == [.merged, .muted, .muted, .failed, .waiting, .success, .quiet])
    }

    @Test("a PR's icon shows its state as GitHub does, whatever its checks")
    func glyph() {
        #expect(PullRequestText.glyph(state: "OPEN", draft: false).tone == .success)
        #expect(PullRequestText.glyph(state: "OPEN", draft: true).tone == .muted)
        #expect(PullRequestText.glyph(state: "MERGED", draft: false).tone == .merged)
        #expect(PullRequestText.glyph(state: "CLOSED", draft: false).tone == .failed)
    }

    @Test("without an agent the PR is titled by its first commit and lists them all")
    func fromCommits() {
        #expect(PullRequestText.fromCommits([CommitRef(sha: "a", subject: "Fix the race")]) == ("Fix the race", ""))
        let (title, body) = PullRequestText.fromCommits([CommitRef(sha: "b", subject: "Add a test"), CommitRef(sha: "a", subject: "Fix the race")])
        #expect(title == "Fix the race" && body == "- Fix the race\n- Add a test")
    }

    @Test("the create button says draft when it makes one, and what gh does while it works")
    func createLabel() {
        #expect(PullRequestText.createLabel(draft: false, busy: nil) == "Create PR")
        #expect(PullRequestText.createLabel(draft: true, busy: nil) == "Create draft PR")
        #expect(PullRequestText.createLabel(draft: true, busy: "Creating draft PR…") == "Creating draft PR…")
    }
}

@Suite("Review threads")
struct ThreadGroupsTests {
    @Test("a thread reads as replied once someone else answers")
    func state() {
        #expect(ThreadState(thread("a", authors: ["mira"])) == .open)
        #expect(ThreadState(thread("a", authors: ["mira", "mira"])) == .open)
        #expect(ThreadState(thread("a")) == .replied)
        #expect(ThreadState(thread("a", resolved: true)) == .resolved)
    }

    @Test("threads group by file in the order each file first comes, unresolved only by default")
    func group() {
        let threads = [thread("1", path: "b.rs"), thread("2", path: "a.rs", resolved: true), thread("3", path: "a.rs"), thread("4", path: "b.rs", outdated: true)]
        let open = ThreadGroups.group(threads, ThreadFilter())
        #expect(open.map(\.path) == ["b.rs", "a.rs"])
        #expect(open.map { $0.threads.map(\.id) } == [["1", "4"], ["3"]])
        let all = ThreadGroups.group(threads, ThreadFilter(resolved: true))
        #expect(all.map { $0.threads.map(\.id) } == [["1", "4"], ["2", "3"]])
        let current = ThreadGroups.group(threads, ThreadFilter(outdated: false))
        #expect(current.map { $0.threads.map(\.id) } == [["1"], ["3"]])
        #expect(ThreadGroups.unresolved(threads) == 3)
    }

    @Test("a hunk tail skips its header")
    func hunkTail() {
        #expect(ThreadGroups.hunkTail("@@ -1 +1 @@\n+a") == ["+a"])
        #expect(ThreadGroups.hunkTail("@@ -1,4 +1,4 @@\n a\n b\n-c\n+d\n e") == ["-c", "+d", " e"])
        #expect(ThreadGroups.hunkTail("").isEmpty)
        #expect(ThreadGroups.hunkTail(nil).isEmpty)
    }

    @Test("a thread's place is its path and line, or its path on a whole file")
    func place() {
        #expect(ThreadGroups.place(thread("a")) == "src/a.rs:42")
        #expect(ThreadGroups.place(ReviewThread(id: "f", path: "README.md", outdated: false, resolved: false, comments: [])) == "README.md")
        #expect(ThreadGroups.replyPrompt(thread("a")) == "Reply to mira…")
    }
}

@Suite("pocketd's PR")
struct GoldenPrTests {
    @Test("the PR pocketd sends decodes with every field the card reads")
    func golden() throws {
        let url = URL(fileURLWithPath: #filePath).deletingLastPathComponent()
            .appending(path: "../../../../../pocketd/internal/github/testdata/golden/cli_pr.json").standardized
        let reply = try JSONDecoder().decode(GhPr.self, from: Data(contentsOf: url))
        let pr = try #require(reply.pr)
        #expect(pr.gate == .checksRunning)
        #expect(PullRequestText.verdict(pr) == (.waiting, "1 check in progress"))
        #expect(PullRequestText.checkRows(pr).named.map(\.name) == ["lint"])
        #expect(ThreadGroups.group(pr.threads, ThreadFilter()).map(\.path) == ["src/sync.rs"])
        #expect(ThreadGroups.hunkTail(pr.threads[0].comments[0].hunk) == [" fn sync() {", "+    loop {", "+        retry();"])
    }
}

@Suite("Gallery")
@MainActor
struct GitHubGalleryTests {
    @Test("every screen has a light and a dark scene")
    func scenes() {
        let names = GitHubGallery.scenes.map(\.name)
        #expect(names.count == Set(names).count)
        #expect(names.filter { $0.hasSuffix("-dark") }.count * 2 == names.count)
        #expect(names.contains("github-pr-failing") && names.contains("github-threads-all-dark"))
    }
}
