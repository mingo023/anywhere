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
        let cleanup = WorktreeCleanup(name: "fix-retry-cap", changes: 0, sessions: 0)
        let screens: [(String, @MainActor () -> AnyView)] = [
            ("pr-failing", { pr(.init(tree: tree, pr: Sample.pullRequest)) }),
            ("pr-ready", { pr(.init(tree: tree, pr: Sample.readyPullRequest), cleanup: cleanup) }),
            ("pr-draft", { pr(.init(tree: tree, pr: Sample.draftPullRequest)) }),
            ("pr-conflicts", { pr(.init(tree: tree, pr: Sample.conflictingPullRequest)) }),
            ("pr-merged", { pr(.init(tree: tree, pr: Sample.mergedPullRequest), cleanup: WorktreeCleanup(name: "fix-retry-cap", changes: 3, sessions: 1)) }),
            ("pr-stale", { pr(.init(tree: tree, pr: Sample.pullRequest, stale: true)) }),
            ("pr-offline", { pr(.init(tree: tree, pr: Sample.readyPullRequest), mac: PrMac(id: Sample.mac, name: "Mingo's MacBook", offline: true)) }),
            ("pr-none", { pr(.init(tree: tree), newPr: newPr) }),
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
            ("pr-create", { AnyView(CreatePullRequestSheet(store: GitHubStore(), tree: tree, newPr: newPr)) }),
            ("pr-list", { list(GhPrs(items: Sample.pullRequests)) }),
            ("pr-list-signed-out", { list(GhPrs(items: Sample.pullRequests, problem: .signedOut)) }),
        ]
        return screens.flatMap { name, view in
            [GalleryScene("github-\(name)", view: view), GalleryScene("github-\(name)-dark", dark: true, view: view)]
        }
    }

    @MainActor private static func pr(_ answer: GhPr, mac: PrMac = PrMac(), newPr: NewPr = NewPr(), cleanup: WorktreeCleanup? = nil) -> AnyView {
        let store = GitHubStore()
        store.show(answer.tree, answer)
        let delete: (@MainActor () async throws -> Void)? = cleanup == nil ? nil : { @MainActor in }
        return AnyView(NavigationStack {
            PullRequestScreen(store: store, tree: answer.tree, mac: mac, newPr: newPr, cleanup: cleanup, deleteTree: delete, every: nil)
        })
    }

    @MainActor private static func threads(_ pr: PullRequest, now: Int, filter: ThreadFilter) -> AnyView {
        let store = GitHubStore()
        store.show(Sample.tree, GhPr(tree: Sample.tree, pr: pr))
        return AnyView(NavigationStack { ThreadsScreen(store: store, model: store.pullRequest(Sample.tree), now: now, filter: filter) })
    }

    @MainActor private static func list(_ list: GhPrs) -> AnyView {
        let store = GitHubStore()
        store.showList("/Users/mingo/dev/anywhere", list)
        return AnyView(NavigationStack {
            PullRequestListScreen(store: store, mac: Sample.mac, project: "/Users/mingo/dev/anywhere", load: false)
        })
    }
}
