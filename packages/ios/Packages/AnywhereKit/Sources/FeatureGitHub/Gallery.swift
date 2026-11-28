import DesignSystem
import Fixtures
import Stores
import SwiftUI
import Wire

public enum GitHubGallery {
    @MainActor public static var scenes: [GalleryScene] {
        let tree = Sample.tree
        let now = Sample.now / 1000
        let newPr = NewPr(branch: "fix-retry-cap", base: "origin/main", ahead: 2, commits: [
            CommitRef(sha: "2222222", subject: "Add a test for the cap"), CommitRef(sha: "1111111", subject: "Cap sync retries at five"),
        ])
        let frames: [(String, @MainActor () -> AnyView)] = [
            ("19-prs-needs-action", { home(Frames.macs()) }),
            ("20-prs-empty", { home(Frames.macs(lists: false)) }),
            ("21-prs-gh-not-signed-in", { home(Frames.macs(studioSignedOut: true)) }),
            ("22-prs-unreachable", { home(Frames.macs(unreachable: true)) }),
            ("23-pr-failing", { pr(Frames.failing, mac: Frames.macBook(), newPr: Frames.failingBranch) }),
            ("23b-pr-reconnecting", {
                pr(Frames.failing, mac: Frames.macBook(.reconnecting), newPr: Frames.failingBranch, strip: NetStrip(.reconnecting, NetStrip.reconnecting(mac: "MacBook Pro")))
            }),
            ("23c-pr-back-online", { pr(Frames.failing, mac: Frames.macBook(), newPr: Frames.failingBranch, strip: NetStrip(.back, "Back online")) }),
            ("23d-pr-reconnect-failed", {
                pr(Frames.failing, mac: Frames.macBook(.unreachable), newPr: Frames.failingBranch,
                   strip: NetStrip(.failed, NetStrip.failed(mac: "MacBook Pro", asOf: nil) + " · showing data from 9:39", action: ("Retry", {})))
            }),
            ("24-pr-ready-to-merge", {
                pr(Frames.ready, mac: Frames.macBook(), newPr: Frames.readyBranch, cleanup: WorktreeCleanup(name: "react-19", changes: 0, sessions: 0))
            }),
            ("25-pr-merged", {
                pr(Frames.merged, mac: Frames.macBook(), newPr: Frames.mergedBranch, cleanup: WorktreeCleanup(name: "upload-limit", changes: 0, sessions: 0))
            }),
            ("26-create-pr", { AnyView(CreatePullRequestSheet(store: GitHubStore(), tree: tree, newPr: Frames.newPr)) }),
        ]
        let screens: [(String, @MainActor () -> AnyView)] = [
            ("pr-draft", { pr(.init(tree: tree, pr: Sample.draftPullRequest)) }),
            ("pr-conflicts", { pr(.init(tree: tree, pr: Sample.conflictingPullRequest)) }),
            ("pr-watch-only", { pr(.init(tree: tree, pr: Sample.pullRequest), fix: FixContext(mac: Sample.mac)) }),
            ("pr-stale", { pr(.init(tree: tree, pr: Sample.pullRequest, stale: true)) }),
            ("pr-none", { pr(.init(tree: tree), newPr: newPr) }),
            ("pr-none-no-commits", { pr(.init(tree: tree), newPr: NewPr(branch: "fix-retry-cap", base: "origin/main", ahead: 0)) }),
            ("pr-no-remote", { pr(.init(tree: tree, problem: .noRemote)) }),
            ("pr-unreachable", { pr(.init(tree: tree, problem: .failed)) }),
            ("pr-gh-missing", { pr(.init(tree: tree, problem: .missing)) }),
            ("pr-gh-signed-out", { pr(.init(tree: tree, problem: .signedOut)) }),
            ("threads-unresolved", { threads(Sample.pullRequest, now: now, filter: ThreadFilter()) }),
            ("threads-all", { threads(Sample.pullRequest, now: now, filter: ThreadFilter(resolved: true)) }),
            ("threads-none-open", { threads(Sample.readyPullRequest, now: now, filter: ThreadFilter()) }),
            ("threads-outdated-hidden", {
                threads(Sample.pullRequest { $0.threads = $0.threads.filter(\.outdated) }, now: now, filter: ThreadFilter(outdated: false))
            }),
            ("pr-list", { list(GhPrs(items: Sample.pullRequests)) }),
            ("pr-list-signed-out", { list(GhPrs(items: Sample.pullRequests, problem: .signedOut)) }),
        ]
        let named = frames.map { ($0.0, $0.1) } + screens.map { ("github-\($0.0)", $0.1) }
        return named.flatMap { name, view in
            [GalleryScene("\(name)-light", view: view), GalleryScene("\(name)-dark", dark: true, view: view)]
        }
    }

    /// The component sheet's PR rows (frame 42): one failing its checks, with Fix, and one merged.
    @MainActor public static var sheetRows: some View {
        let failing = PrItem(
            tree: "/Users/minh/.worktrees/anywhere/flaky-checkout", number: 42, title: "Fix flaky checkout test", state: "OPEN", draft: false, gate: .checksFailed,
            checks: Checks(passed: 2, failed: 2, pending: 0))
        let merged = PrItem(
            tree: "/Users/minh/.worktrees/fox-api/upload-limit", number: 57, title: "Rate-limit the upload route", state: "MERGED", draft: false, gate: .done,
            checks: Checks(passed: 4, failed: 0, pending: 0))
        func entry(_ repo: String, _ item: PrItem) -> PrEntry {
            PrEntry(mac: "macbook", macName: Frames.macBookName, projectPath: "/Users/minh/dev/\(repo)", repo: repo, item: item)
        }
        return VStack(spacing: 0) {
            PrRow(entry: entry("anywhere", failing), action: .of(failing)) { _ in }
            PrRow(entry: entry("fox-api", merged), action: .of(merged)) { _ in }.rowDivider(true, leading: PrRow.textColumn)
        }
        .insetGroup()
    }

    /// A worktree with one Claude session, on a phone that may prompt it.
    @MainActor private static var fix: FixContext {
        FixContext(mac: Sample.mac, target: .one(ChatSession(id: "a-working", title: "Port the commit graph lanes", agent: "Claude")),
                   canDrive: true, send: { _, _ in true })
    }

    /// `strip` is the banner the shell puts over a pushed screen, drawn here as it does.
    @MainActor private static func pr(
        _ answer: GhPr, mac: PrMac = PrMac(), newPr: NewPr = NewPr(), cleanup: WorktreeCleanup? = nil, fix: FixContext = fix, strip: NetStrip? = nil
    ) -> AnyView {
        let store = GitHubStore()
        store.show(answer.tree, answer)
        let delete: (@MainActor () -> Void)? = cleanup == nil ? nil : { @MainActor in }
        return AnyView(NavigationStack {
            PullRequestScreen(store: store, tree: answer.tree, mac: mac, newPr: newPr, cleanup: cleanup, deleteTree: delete, fix: fix, every: nil)
                .safeAreaInset(edge: .top, spacing: 0) {
                    if let strip { strip.padding(.horizontal, Inset.margin).padding(.vertical, Space.xs) }
                }
        })
    }

    @MainActor private static func threads(_ pr: PullRequest, now: Int, filter: ThreadFilter) -> AnyView {
        let store = GitHubStore()
        store.show(Sample.tree, GhPr(tree: Sample.tree, pr: pr))
        return AnyView(NavigationStack { ThreadsScreen(store: store, model: store.pullRequest(Sample.tree), now: now, filter: filter, fix: fix) })
    }

    @MainActor private static func list(_ list: GhPrs) -> AnyView {
        let store = GitHubStore()
        store.showList("/Users/mingo/dev/anywhere", list)
        return AnyView(NavigationStack {
            PullRequestListScreen(store: store, mac: Sample.mac, project: "/Users/mingo/dev/anywhere", load: false)
        })
    }

    @MainActor private static func home(_ hosts: [HostModel]) -> AnyView {
        AnyView(NavigationStack { PullRequestsHome(hosts: hosts, load: false) })
    }
}
