import Wire

extension Sample {
    /// A tree's open PR held back by a failed check and requested changes, with threads on two files.
    public static let pullRequest = PullRequest(
        number: 118, title: "Cap sync retries at five", url: "https://github.com/mingo023/anywhere/pull/118", state: "OPEN", draft: false,
        base: "main", head: "5f85ec828f6d2fe28d81ba71a61b8a2181edfb6a", checks: Checks(passed: 5, failed: 1, pending: 1),
        runs: [
            CheckRun(name: "build", outcome: "passed", url: "https://github.com/mingo023/anywhere/actions/runs/1/job/1"),
            CheckRun(name: "test (macos-14)", outcome: "failed", url: "https://github.com/mingo023/anywhere/actions/runs/1/job/2"),
            CheckRun(name: "lint", outcome: "pending", url: "https://github.com/mingo023/anywhere/actions/runs/1/job/3"),
            CheckRun(name: "clippy", outcome: "passed", url: "https://github.com/mingo023/anywhere/actions/runs/1/job/4"),
            CheckRun(name: "type-label", outcome: "skipped"),
            CheckRun(name: "e2e", outcome: "passed", url: "https://github.com/mingo023/anywhere/actions/runs/1/job/5"),
            CheckRun(name: "vercel", outcome: "passed", url: "https://vercel.com/mingo/anywhere/abc"),
        ],
        review: "CHANGES_REQUESTED",
        reviews: [Review(login: "rina", state: "CHANGES_REQUESTED"), Review(login: "bo", state: "APPROVED"), Review(login: "kai", state: "COMMENTED")],
        requested: ["jo"], mergeable: "MERGEABLE", gate: .checksFailed,
        threads: [
            ReviewThread(id: "PRRT_a", path: "packages/pocketd/internal/sync/retry.go", line: 42, outdated: false, resolved: false, comments: [
                ThreadComment(
                    author: "rina", body: "This retries **forever** when the relay is down. Can we cap it, say at `5`, and back off?",
                    at: 1_792_820_000, hunk: " func sync() {\n+\tfor {\n+\t\tif err := retry(); err == nil {\n+\t\t\treturn"),
            ]),
            ReviewThread(id: "PRRT_b", path: "packages/pocketd/internal/sync/retry.go", line: 57, outdated: false, resolved: false, comments: [
                ThreadComment(author: "rina", body: "Nit: name this `backoff`.", at: 1_792_820_100, hunk: "+\twait := time.Second\n+\tfor range 5 {"),
                ThreadComment(author: "mingo", body: "Renamed in 1a2b3c4.", at: 1_792_826_000),
            ]),
            ReviewThread(id: "PRRT_c", path: "README.md", line: 7, left: true, outdated: true, resolved: true, comments: [
                ThreadComment(author: "bo", body: "Typo: *recieve*.", at: 1_792_800_000, hunk: "-Phones recieve a push\n+Phones receive a push"),
            ]),
            ReviewThread(id: "PRRT_d", path: "docs/mobile/protocol.md", line: 211, outdated: true, resolved: false, comments: [
                ThreadComment(
                    author: "kai", body: "Should `gh.changed` carry the gate too?\n\n- saves a round trip\n- but grows every push",
                    at: 1_792_810_000, hunk: " ← {type:\"gh.changed\", tree}"),
            ]),
        ])

    public static func pullRequest(_ edit: (inout PullRequest) -> Void) -> PullRequest {
        var pr = pullRequest
        edit(&pr)
        return pr
    }

    public static let readyPullRequest = pullRequest {
        $0.checks = Checks(passed: 6, failed: 0, pending: 0)
        $0.runs = $0.runs.map { CheckRun(name: $0.name, outcome: $0.outcome == "skipped" ? "skipped" : "passed", url: $0.url) }
        $0.review = "APPROVED"
        $0.reviews = [Review(login: "rina", state: "APPROVED"), Review(login: "bo", state: "APPROVED")]
        $0.requested = []
        $0.gate = .ready
        $0.threads = $0.threads.map { var t = $0; t.resolved = true; return t }
    }

    public static let draftPullRequest = pullRequest {
        $0.number = 121
        $0.title = "Project settings lead with the project's icon"
        $0.draft = true
        $0.checks = Checks(passed: 0, failed: 0, pending: 2)
        $0.runs = [CheckRun(name: "build", outcome: "pending"), CheckRun(name: "test", outcome: "pending")]
        $0.review = nil
        $0.reviews = []
        $0.requested = []
        $0.mergeable = "UNKNOWN"
        $0.gate = .draft
        $0.threads = []
    }

    public static let mergedPullRequest = pullRequest {
        $0.number = 119
        $0.title = "A chat switches account by hand or at a limit"
        $0.state = "MERGED"
        $0.gate = .done
        $0.checks = Checks(passed: 6, failed: 0, pending: 0)
        $0.runs = []
        $0.review = "APPROVED"
        $0.reviews = [Review(login: "rina", state: "APPROVED")]
        $0.requested = []
        $0.threads = []
    }

    public static let conflictingPullRequest = pullRequest {
        $0.number = 120
        $0.title = "Sidebar rows pin only from their menu"
        $0.mergeable = "CONFLICTING"
        $0.gate = .conflicts
    }

    public static let pullRequests: [PrItem] = [
        PrItem(tree: tree, number: 118, title: pullRequest.title, state: "OPEN", draft: false, gate: .checksFailed, checks: pullRequest.checks),
        PrItem(tree: "/Users/mingo/.worktrees/anywhere/fix-login", number: 121, title: draftPullRequest.title, state: "OPEN", draft: true,
               gate: .draft, checks: draftPullRequest.checks),
        PrItem(tree: "/Users/mingo/.worktrees/anywhere/quiet-heron", number: 120, title: conflictingPullRequest.title, state: "OPEN", draft: false,
               gate: .conflicts, checks: Checks(passed: 3, failed: 0, pending: 0)),
        PrItem(tree: "/Users/mingo/.worktrees/anywhere/brave-lynx", number: 117, title: "Changelog for 0.1.13", state: "OPEN", draft: false,
               gate: .ready, checks: Checks(passed: 6, failed: 0, pending: 0)),
        PrItem(tree: "/Users/mingo/.worktrees/anywhere/swift-wren", number: 119, title: mergedPullRequest.title, state: "MERGED", draft: false,
               gate: .done, checks: Checks(passed: 6, failed: 0, pending: 0)),
    ]
}
