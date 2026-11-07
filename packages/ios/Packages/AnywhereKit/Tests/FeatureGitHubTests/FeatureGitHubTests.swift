import ChatModel
import Fixtures
import Foundation
import Stores
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
    @Test("a PR is offered only for a branch off its base")
    func offersCreate() {
        #expect(PullRequestText.offersCreate(branch: "fix", base: "main"))
        #expect(!PullRequestText.offersCreate(branch: "fix", base: nil))
        #expect(!PullRequestText.offersCreate(branch: "main", base: "origin/main"))
        #expect(!PullRequestText.offersCreate(branch: nil, base: "origin/main"))
        #expect(PullRequestText.baseBranch("origin/main") == "main")
        #expect(PullRequestText.baseBranch("develop") == "develop")
    }

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

    @Test("a new PR's top line names its branch into its base and how far ahead it is")
    func headLine() {
        #expect(PullRequestText.headLine(branch: "fix", base: "origin/main", ahead: 3) == "fix → main · 3 commits ahead")
        #expect(PullRequestText.headLine(branch: "fix", base: "main", ahead: 1) == "fix → main · 1 commit ahead")
        #expect(PullRequestText.headLine(branch: "fix", base: "develop", ahead: nil) == "fix → develop")
        #expect(NewPr(branch: "fix", base: "origin/main", ahead: 2).line == "fix → main · 2 commits ahead")
        #expect(NewPr(branch: nil, base: "main").line == nil && !NewPr(branch: nil, base: "main").offered)
    }

    @Test("a merge method is remembered for the PR's repository")
    func repo() {
        #expect(PullRequestText.repo("https://github.com/acme/app/pull/12") == "acme/app")
        #expect(PullRequestText.repo("not a url") == nil)
    }

    @Test("a reply names the gh account it posts as, and the Mac it posts from")
    func postsAs() {
        #expect(PullRequestText.postsAs(login: "mingo023", mac: "Studio") == "Posts to GitHub as @mingo023 from Studio")
        #expect(PullRequestText.postsAs(login: nil, mac: "Studio") == "Posts to GitHub as Studio's gh account")
    }

    @Test("gh's problems say what to run on which Mac")
    func problemText() {
        #expect(GhProblemView.text(.signedOut, mac: "Studio") == ("GitHub CLI isn't signed in", "On Studio, run gh auth login."))
        #expect(GhProblemView.text(.missing, mac: "Studio").detail == "On Studio, run brew install gh, then gh auth login.")
    }

    @Test("the callout counts what there is to fix")
    func fixSummary() {
        #expect(PullRequestText.fixSummary(checks: 1, comments: 2) == ("3 things to fix", "1 failed check · 2 review comments"))
        #expect(PullRequestText.fixSummary(checks: 0, comments: 1) == ("1 thing to fix", "1 review comment"))
        #expect(PullRequestText.resolving("Claude") == "Claude is resolving the conflict")
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
        #expect(ThreadState(thread("a"), sent: true) == .sent)
        #expect(ThreadState(thread("a", resolved: true), sent: true) == .resolved)
    }

    @Test("what can still be sent is each failed check and open thread the agent isn't fixing")
    func unsent() {
        let p = pr {
            $0.runs = [check("a", "failed"), check("b", "passed"), check("c", "failed")]
            $0.threads = [thread("1", authors: ["mira"]), thread("2"), thread("3", resolved: true), thread("4", authors: ["kai"])]
        }
        let all = ThreadGroups.unsent(p, fixing: nil)
        #expect(all.checks.map(\.name) == ["a", "c"] && all.threads.map(\.id) == ["1", "4"])
        var fixing = SentFixes(head: "aaa")
        fixing.checks = ["a"]
        fixing.threads = ["1"]
        let rest = ThreadGroups.unsent(p, fixing: fixing)
        #expect(rest.checks.map(\.name) == ["c"] && rest.threads.map(\.id) == ["4"])
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

    @Test("the reply box names whom the reply answers")
    func replyPrompt() {
        #expect(ThreadGroups.replyPrompt(thread("a")) == "Reply to mira…")
    }
}

@Suite("Comments empty states")
struct ThreadsEmptyTests {
    @Test("a PR with no threads has no review comments yet")
    func none() {
        #expect(ThreadGroups.empty([], ThreadFilter()) == .noComments)
    }

    @Test("unresolved threads hidden only for being outdated are counted, never called resolved")
    func outdatedHidden() {
        let threads = [thread("1", outdated: true), thread("2", outdated: true), thread("3", resolved: true)]
        #expect(ThreadGroups.empty(threads, ThreadFilter(outdated: false)) == .outdatedHidden(2))
        #expect(ThreadGroups.empty(threads, ThreadFilter(resolved: true, outdated: false)) == nil)
        #expect(ThreadGroups.empty(threads, ThreadFilter()) == nil)
    }

    @Test("all resolved only when none is unresolved")
    func allResolved() {
        let threads = [thread("1", resolved: true), thread("2", resolved: true, outdated: true)]
        #expect(ThreadGroups.empty(threads, ThreadFilter(outdated: false)) == .allResolved)
        #expect(ThreadGroups.empty(threads, ThreadFilter(resolved: true)) == nil)
    }
}

@Suite("PR screen states")
struct PullRequestPhaseTests {
    @Test("only gh saying there is none shows No pull request, and so offers Create PR")
    func none() {
        #expect(PullRequestPhase.of(hasPr: false, problem: nil, loaded: true, error: nil) == .none)
        #expect(PullRequestPhase.of(hasPr: false, problem: .failed, loaded: true, error: nil) == .unreachable)
        #expect(PullRequestPhase.of(hasPr: false, problem: .noRemote, loaded: true, error: nil) == .noRemote)
        #expect(PullRequestPhase.of(hasPr: false, problem: "somethingNew", loaded: true, error: nil) == .unreachable)
    }

    @Test("gh missing or signed out each say so, and a PR shows whatever else is wrong")
    func problems() {
        #expect(PullRequestPhase.of(hasPr: false, problem: .missing, loaded: true, error: nil) == .ghMissing)
        #expect(PullRequestPhase.of(hasPr: false, problem: .signedOut, loaded: true, error: nil) == .ghSignedOut)
        #expect(PullRequestPhase.of(hasPr: true, problem: .failed, loaded: true, error: "x") == .card)
    }

    @Test("before any answer, a failed ask shows its reason and otherwise it loads")
    func first() {
        #expect(PullRequestPhase.of(hasPr: false, problem: nil, loaded: false, error: "Not connected") == .failed("Not connected"))
        #expect(PullRequestPhase.of(hasPr: false, problem: nil, loaded: false, error: nil) == .loading)
    }
}

@Suite("Worktree after a merge")
struct WorktreeCleanupTests {
    @Test("deleting after the merge starts on only for a clean worktree with no sessions")
    func byDefault() {
        #expect(WorktreeCleanup(name: "w", changes: 0, sessions: 0).deletesByDefault)
        #expect(!WorktreeCleanup(name: "w", changes: 2, sessions: 0).deletesByDefault)
        #expect(!WorktreeCleanup(name: "w", changes: 0, sessions: 1).deletesByDefault)
        #expect(!WorktreeCleanup(name: "w", changes: nil, sessions: 0).deletesByDefault)
    }

    @Test("deleting asks first unless the worktree is known to be clean")
    func asks() {
        #expect(!WorktreeCleanup(name: "w", changes: 0, sessions: 3).asks)
        #expect(WorktreeCleanup(name: "w", changes: 1, sessions: 0).asks)
        #expect(WorktreeCleanup(name: "w", changes: nil, sessions: 0).asks)
        #expect(WorktreeCleanup(name: "w", changes: 3, sessions: 0).confirmTitle == "Delete w and its 3 uncommitted changes?")
        #expect(WorktreeCleanup(name: "w", changes: 1, sessions: 0).confirmTitle == "Delete w and its uncommitted change?")
        #expect(WorktreeCleanup(name: "w", changes: nil, sessions: 0).confirmTitle == "Delete w?")
    }

    @Test("it names what closes with the worktree")
    func closes() {
        #expect(WorktreeCleanup(name: "w", changes: 0, sessions: 0).closes == "Its terminals close; the branch stays.")
        #expect(WorktreeCleanup(name: "w", changes: 0, sessions: 1).closes == "Its session and terminals close; the branch stays.")
        #expect(WorktreeCleanup(name: "w", changes: 0, sessions: 2).closes == "Its 2 sessions and terminals close; the branch stays.")
    }

    @Test("a session is in the worktree when its folder is the worktree or inside it")
    func sessions() {
        let cwds = ["/w/fix", "/w/fix/src", "/w/fix-two", "/w"]
        #expect(WorktreeCleanup.sessions(in: "/w/fix", cwds: cwds) == 2)
    }
}

@Suite("Sending a PR's problems")
struct FixTargetTests {
    let claude = ChatSession(id: "a", title: "Fix the build", agent: "Claude")
    let codex = ChatSession(id: "b", title: "Docs", agent: "Codex")

    @Test("the buttons name the session's agent, the agent all the choices share, else agent")
    func agent() {
        #expect(ChatTarget.one(claude).title("Send to") == "Send to Claude")
        #expect(ChatTarget.choose([codex, claude]).title("Send all to") == "Send all to agent")
        #expect(ChatTarget.choose([claude, ChatSession(id: "c", title: "", agent: "Claude")]).agent == "Claude")
    }

    @Test("a worktree without a session offers to start one")
    func none() {
        #expect(ChatTarget.none.agent == nil)
        #expect(ChatTarget.none.title("Send to") == "Start a session with this")
    }

    @Test("nothing is offered without the drive scope, and a worktree without a session needs a way to start one")
    @MainActor
    func offered() {
        let one = ChatTarget.one(claude)
        #expect(FixContext(target: one, canDrive: true, send: { _, _ in true }).offered)
        #expect(!FixContext(target: one, canDrive: false, send: { _, _ in true }).offered)
        #expect(!FixContext(target: .none, canDrive: true).offered)
        #expect(FixContext(target: .none, canDrive: true, open: { _ in }).offered)
    }

    @Test("a thread is offered to the agent only while it isn't fixing anything on the PR")
    @MainActor
    func threadWhileFixing() {
        let fix = FixContext(target: .one(claude), canDrive: true, send: { _, _ in true })
        var fixing = SentFixes(head: "h")
        fixing.conflict = true
        #expect(fix.canSend(fixing: nil))
        #expect(!fix.canSend(fixing: fixing))
        #expect(!FixContext(target: .one(claude), canDrive: false, send: { _, _ in true }).canSend(fixing: nil))
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
        #expect(FixPrompt.hunkTail(pr.threads[0].comments[0].hunk) == [" fn sync() {", "+    loop {", "+        retry();"])
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
