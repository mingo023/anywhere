import Fixtures
import Foundation
import Stores
import Wire

/// The design's PR frames (19 to 26) as data: Minh's MacBook Pro and Studio Mac Mini, with anywhere, garden-mobile and fox-api.
@MainActor
enum Frames {
    static let macBookName = "Minh's MacBook Pro"

    static func macBook(_ reachability: Reachability = .online) -> PrMac {
        PrMac(id: "macbook", name: macBookName, reachability: reachability)
    }

    /// The PRs tab on two Macs. `lists` false leaves every list empty; `studioSignedOut` gives the Mini a gh that isn't signed in;
    /// `unreachable` has gh failing to reach GitHub for the MacBook's last project.
    static func macs(lists: Bool = true, studioSignedOut: Bool = false, unreachable: Bool = false) -> [HostModel] {
        let macBook = host(id: "macbook", name: macBookName, projects: ["anywhere", "garden-mobile", "fox-api"])
        let anywhere = lists ? [
            item(42, "Fix flaky checkout test", gate: .checksFailed, checks: Checks(passed: 2, failed: 2, pending: 0)),
            item(214, "Migrate to React 19", gate: .ready, checks: Checks(passed: 4, failed: 0, pending: 0)),
            item(219, "Add dark mode to settings", draft: true, gate: .draft, checks: Checks(passed: 0, failed: 0, pending: 1)),
        ] : []
        let garden = lists ? [item(88, "Fix crash on resume", gate: .reviewRequired, checks: Checks(passed: 3, failed: 0, pending: 0))] : []
        let fox = lists ? [item(57, "Rate-limit the upload route", state: "MERGED", gate: .done, checks: Checks(passed: 4, failed: 0, pending: 0))] : []
        macBook.github.showList("/Users/minh/dev/anywhere", GhPrs(items: anywhere))
        macBook.github.showList("/Users/minh/dev/garden-mobile", GhPrs(items: garden))
        macBook.github.showList("/Users/minh/dev/fox-api", GhPrs(items: fox, problem: unreachable ? .failed : nil))
        guard studioSignedOut else { return [macBook] }
        let studio = host(id: "studio", name: "Studio Mac Mini", projects: ["hub"])
        studio.github.showList("/Users/minh/dev/hub", GhPrs(items: [], problem: .signedOut))
        return [studio, macBook]
    }

    private static func host(id: String, name: String, projects: [String]) -> HostModel {
        var hello = Sample.hello
        hello.serverId = id
        hello.hostname = name
        let list = ProjectList(projects: projects.map { Project(path: "/Users/minh/dev/\($0)", name: $0, worktrees: []) })
        return HostModel.offline(hello: hello, messages: [.projectList(list)])
    }

    private static func item(_ number: Int, _ title: String, state: String = "OPEN", draft: Bool = false, gate: Gate, checks: Checks) -> PrItem {
        PrItem(tree: "/Users/minh/.worktrees/\(number)", number: number, title: title, state: state, draft: draft, gate: gate, checks: checks)
    }

    static let failingBranch = NewPr(branch: "flaky-checkout", base: "origin/main", ahead: 3)
    static let readyBranch = NewPr(branch: "react-19", base: "origin/main", ahead: 14)
    static let mergedBranch = NewPr(branch: "upload-limit", base: "origin/main", ahead: 0)

    /// What gives the sheet the design's title and a description from the commits.
    static let newPr = NewPr(branch: "flaky-checkout", base: "origin/main", ahead: 2, commits: [
        CommitRef(sha: "2222222", subject: "Await server.listen() so the test no longer races the handler"),
        CommitRef(sha: "1111111", subject: "Fix flaky checkout test"),
    ])

    static let failing = GhPr(tree: Sample.tree, pr: PullRequest(
        number: 42, title: "Fix flaky checkout test", url: "https://github.com/minh/anywhere/pull/42", state: "OPEN", draft: false, base: "main",
        head: "a1c9e3f", checks: Checks(passed: 2, failed: 2, pending: 0),
        runs: [
            CheckRun(name: "test / checkout", outcome: "failed", url: "https://github.com/minh/anywhere/actions/runs/1"),
            CheckRun(name: "test / e2e", outcome: "failed", url: "https://github.com/minh/anywhere/actions/runs/2"),
            CheckRun(name: "build", outcome: "passed"), CheckRun(name: "lint", outcome: "passed"),
        ],
        review: "REVIEW_REQUIRED", reviews: [], requested: ["Lan Pham"], mergeable: "MERGEABLE", gate: .checksFailed, threads: []))

    static let ready = GhPr(tree: Sample.tree, pr: PullRequest(
        number: 214, title: "Migrate to React 19", url: "https://github.com/minh/anywhere/pull/214", state: "OPEN", draft: false, base: "main",
        head: "b2d4f60", checks: Checks(passed: 4, failed: 0, pending: 0),
        runs: ["build", "lint", "test", "e2e"].map { CheckRun(name: $0, outcome: "passed") },
        review: "APPROVED", reviews: [Review(login: "Lan Pham", state: "APPROVED")], requested: [], mergeable: "MERGEABLE", gate: .ready,
        threads: [ReviewThread(id: "t1", path: "package.json", line: 3, outdated: false, resolved: true, comments: [
            ThreadComment(author: "Lan Pham", body: "Ship it after the release cut.", at: Int(Date().timeIntervalSince1970) - 38 * 60),
        ])]))

    static let merged = GhPr(tree: Sample.tree, pr: PullRequest(
        number: 57, title: "Rate-limit the upload route", url: "https://github.com/minh/fox-api/pull/57", state: "MERGED", draft: false, base: "main",
        head: "c3e5a71", checks: Checks(passed: 4, failed: 0, pending: 0), runs: [], review: "APPROVED",
        reviews: [Review(login: "Lan Pham", state: "APPROVED")], requested: [], mergeable: "UNKNOWN", gate: .done, threads: []))
}
