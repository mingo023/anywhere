import Fixtures
import Stores
import Testing
import Wire
@testable import FeatureGitHub

private func item(
    _ number: Int, state: String = "OPEN", draft: Bool = false, gate: Gate = .reviewRequired, passed: Int = 0, failed: Int = 0, pending: Int = 0
) -> PrItem {
    PrItem(tree: "/t/\(number)", number: number, title: "PR \(number)", state: state, draft: draft, gate: gate,
           checks: Checks(passed: passed, failed: failed, pending: pending))
}

private func entry(_ number: Int, state: String = "OPEN", gate: Gate = .reviewRequired) -> PrEntry {
    PrEntry(mac: "m1", macName: "Mac", projectPath: "/p/anywhere", repo: "anywhere", item: item(number, state: state, gate: gate))
}

@Suite("PRs tab groups")
struct PullRequestSectionsTests {
    @Test("a PR lists by what it wants of the reader")
    func sectionOf() {
        #expect(PrSection.of(item(1, gate: .checksFailed, failed: 1)) == .needsAction)
        #expect(PrSection.of(item(2, gate: .conflicts)) == .needsAction)
        #expect(PrSection.of(item(3, gate: .changesRequested)) == .needsAction)
        #expect(PrSection.of(item(4, gate: .ready, passed: 3)) == .readyToMerge)
        #expect(PrSection.of(item(5, gate: .checksRunning, pending: 1)) == .open)
        #expect(PrSection.of(item(6, gate: .reviewRequired, passed: 2)) == .open)
        #expect(PrSection.of(item(7, state: "MERGED", gate: .done)) == .mergedRecently)
    }

    @Test("a draft is open however its checks went, and a closed PR lists nowhere")
    func draftAndClosed() {
        #expect(PrSection.of(item(1, draft: true, gate: .draft, failed: 1)) == .open)
        #expect(PrSection.of(item(2, state: "CLOSED", gate: .done)) == nil)
    }

    @Test("groups come in the design's order, skip the empty ones and keep the order the PRs came in")
    func groups() {
        let entries = [
            entry(1, state: "MERGED"), entry(2, gate: .ready), entry(3, gate: .conflicts), entry(4, gate: .checksFailed), entry(5, gate: .checksRunning),
            entry(6, state: "CLOSED"),
        ]
        let groups = PrSections.group(entries)
        #expect(groups.map(\.section) == [.needsAction, .readyToMerge, .open, .mergedRecently])
        #expect(groups.first?.entries.map(\.item.number) == [3, 4])
        #expect(PrSections.group([entry(5, gate: .checksRunning)]).map(\.section) == [.open])
        #expect(PrSections.group([]).isEmpty)
    }

    @Test("the tab's badge counts the PRs in Needs action")
    func needingAction() {
        let items = [item(1, gate: .checksFailed, failed: 2), item(2, gate: .ready), item(3, gate: .conflicts), item(4, draft: true, gate: .draft)]
        #expect(PrSections.needingAction(items) == 2)
        #expect(PrSections.needingAction([]) == 0)
    }

    @Test("a row offers Fix when it needs action and Merge when it is ready, and nothing else")
    func rowAction() {
        #expect(PrRowAction.of(item(1, gate: .checksFailed, failed: 1)) == .fix)
        #expect(PrRowAction.of(item(2, gate: .ready)) == .merge)
        #expect(PrRowAction.of(item(3, gate: .reviewRequired)) == nil)
        #expect(PrRowAction.of(item(4, state: "MERGED", gate: .done)) == nil)
    }
}

@Suite("PR row text")
struct PullRequestRowTextTests {
    @Test("only the checks part names a count or a state; the rest is plain words")
    func meta() {
        #expect(PrMeta.of(item(1, gate: .checksFailed, passed: 2, failed: 2)) == PrMeta(checks: .failed(2), detail: nil))
        #expect(PrMeta.of(item(2, gate: .reviewRequired, passed: 3)) == PrMeta(checks: .passed, detail: "Review required"))
        #expect(PrMeta.of(item(3, draft: true, gate: .draft, pending: 2)) == PrMeta(checks: .running, detail: "Draft"))
        #expect(PrMeta.of(item(4, gate: .changesRequested, passed: 1)) == PrMeta(checks: .passed, detail: "Changes requested"))
        #expect(PrMeta.of(item(5, state: "MERGED", gate: .done, passed: 4)) == PrMeta(checks: .passed, detail: "Merged"))
    }

    @Test("failed checks outrank running ones, which outrank passed ones")
    func checksPrecedence() {
        #expect(PrMeta.of(item(1, passed: 5, failed: 1, pending: 1)).checks == .failed(1))
        #expect(PrMeta.of(item(2, passed: 5, pending: 1)).checks == .running)
        #expect(PrMeta.of(item(3, gate: .checking)).isEmpty)
    }

    @Test("a PR that needs action has a red icon and a merged one a merge icon")
    func icon() {
        #expect(PrSection.icon(item(1, gate: .checksFailed, failed: 1)).tone == .failed)
        #expect(PrSection.icon(item(2, gate: .ready)).tone == .success)
        #expect(PrSection.icon(item(3, draft: true, gate: .draft)).icon == .pullRequestDraft)
        #expect(PrSection.icon(item(4, state: "MERGED", gate: .done)).icon == .merge)
    }
}

@Suite("PR tab problems")
struct PullRequestProblemsTests {
    @Test("a Mac whose gh isn't signed in or installed gets one notice naming it")
    func notices() {
        let notices = PrProblems.notices([
            MacPrProblems(id: "a", name: "Studio Mac Mini", problems: [.signedOut, .signedOut]),
            MacPrProblems(id: "b", name: "MacBook", problems: [.ready]),
            MacPrProblems(id: "c", name: "Mini", problems: [.missing]),
        ])
        #expect(notices.map(\.id) == ["a", "c"])
        #expect(notices.first?.title == "GitHub CLI isn't signed in on Studio Mac Mini")
        #expect(notices.first?.detail == "Run gh auth login on the Mac to see its pull requests")
        #expect(notices.last?.title == "GitHub CLI isn't installed on Mini")
    }

    @Test("GitHub being unreachable is a strip, not a notice")
    func unreachable() {
        let macs = [MacPrProblems(id: "a", name: "A", problems: [.failed])]
        #expect(PrProblems.unreachable(macs))
        #expect(PrProblems.notices(macs).isEmpty)
        #expect(!PrProblems.unreachable([MacPrProblems(id: "a", name: "A", problems: [.signedOut])]))
    }
}

@MainActor
@Suite("PRs across Macs")
struct PullRequestsHomeTests {
    private func host(_ name: String, id: String, projects: [(String, String)], lists: [String: [PrItem]]) -> HostModel {
        var hello = Sample.hello
        hello.serverId = id
        hello.hostname = name
        let list = ProjectList(projects: projects.map { Project(path: $0.0, name: $0.1, worktrees: []) })
        let host = HostModel.offline(hello: hello, messages: [.projectList(list)])
        for (path, items) in lists { host.github.showList(path, GhPrs(items: items)) }
        return host
    }

    @Test("the badge counts what needs action on every Mac's projects")
    func badge() {
        let one = host("MacBook", id: "m1", projects: [("/a", "anywhere"), ("/g", "garden-mobile")], lists: [
            "/a": [item(1, gate: .checksFailed, failed: 1), item(2, gate: .ready)], "/g": [item(3, gate: .conflicts)],
        ])
        let two = host("Mini", id: "m2", projects: [("/f", "fox-api")], lists: ["/f": [item(4, gate: .changesRequested)]])
        #expect(PullRequestsHome.needingAction([one, two]) == 3)
        #expect(PullRequestsHome.needingAction([]) == 0)
    }

    @Test("a row is named by its project and its Mac, and opens that Mac's PR")
    func entries() {
        let mac = host("MacBook", id: "m1", projects: [("/a", "anywhere")], lists: ["/a": [item(42, gate: .checksFailed, failed: 1)]])
        let entries = PullRequestsHome.entries([mac])
        #expect(entries.map(\.repo) == ["anywhere"])
        #expect(entries.first?.route == .pullRequest(mac: "m1", tree: "/t/42"))
    }

    @Test("gh problems are read from each Mac's projects")
    func problems() {
        let mac = host("Mini", id: "m2", projects: [("/f", "fox-api")], lists: [:])
        mac.github.showList("/f", GhPrs(items: [], problem: .signedOut))
        #expect(PullRequestsHome.problems([mac]) == [MacPrProblems(id: "m2", name: "Mini", problems: [.signedOut])])
    }
}
