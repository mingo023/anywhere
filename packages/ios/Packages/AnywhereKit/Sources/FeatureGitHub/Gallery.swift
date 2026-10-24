import DesignSystem
import Fixtures
import Stores
import SwiftUI
import Wire

public enum GitHubGallery {
    @MainActor public static var scenes: [GalleryScene] {
        let tree = Sample.tree
        let now = Sample.now / 1000
        let screens: [(String, @MainActor () -> AnyView)] = [
            ("pr-failing", { pr(.init(tree: tree, pr: Sample.pullRequest)) }),
            ("pr-ready", { pr(.init(tree: tree, pr: Sample.readyPullRequest)) }),
            ("pr-draft", { pr(.init(tree: tree, pr: Sample.draftPullRequest)) }),
            ("pr-conflicts", { pr(.init(tree: tree, pr: Sample.conflictingPullRequest)) }),
            ("pr-merged", { pr(.init(tree: tree, pr: Sample.mergedPullRequest)) }),
            ("pr-none", { pr(.init(tree: tree)) }),
            ("pr-gh-missing", { pr(.init(tree: tree, problem: .missing)) }),
            ("pr-gh-signed-out", { pr(.init(tree: tree, problem: .signedOut)) }),
            ("threads-unresolved", { threads(Sample.pullRequest, now: now, filter: ThreadFilter()) }),
            ("threads-all", { threads(Sample.pullRequest, now: now, filter: ThreadFilter(resolved: true)) }),
            ("threads-none-open", { threads(Sample.readyPullRequest, now: now, filter: ThreadFilter()) }),
            ("pr-create", {
                AnyView(CreatePullRequestSheet(store: GitHubStore(), tree: tree, commits: [
                    CommitRef(sha: "2222222", subject: "Add a test for the cap"), CommitRef(sha: "1111111", subject: "Cap sync retries at five"),
                ]))
            }),
            ("pr-list", {
                let store = GitHubStore()
                store.showList("/Users/mingo/dev/anywhere", Sample.pullRequests)
                return AnyView(NavigationStack {
                    PullRequestListScreen(store: store, mac: Sample.mac, project: "/Users/mingo/dev/anywhere", load: false)
                })
            }),
        ]
        return screens.flatMap { name, view in
            [GalleryScene("github-\(name)", view: view), GalleryScene("github-\(name)-dark", dark: true, view: view)]
        }
    }

    @MainActor private static func pr(_ answer: GhPr) -> AnyView {
        let store = GitHubStore()
        store.show(answer.tree, answer)
        return AnyView(NavigationStack { PullRequestScreen(store: store, tree: answer.tree, every: nil) })
    }

    @MainActor private static func threads(_ pr: PullRequest, now: Int, filter: ThreadFilter) -> AnyView {
        let store = GitHubStore()
        store.show(Sample.tree, GhPr(tree: Sample.tree, pr: pr))
        return AnyView(NavigationStack { ThreadsScreen(store: store, model: store.pullRequest(Sample.tree), now: now, filter: filter) })
    }
}
