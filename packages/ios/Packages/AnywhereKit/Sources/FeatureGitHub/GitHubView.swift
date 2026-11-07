import DesignSystem
import Domain
import Stores
import SwiftUI

/// A tree's pull request and a project's pull requests.
@MainActor
public struct GitHubView: View {
    let host: HostModel
    let route: Route

    public init(host: HostModel, route: Route) {
        self.host = host
        self.route = route
    }

    public var body: some View {
        switch route {
        case .pullRequest(let mac, let tree):
            // The tree's status gives the new PR its commits and base, and its watch brings gh.changed.
            let head = host.git.changes(tree).head
            PullRequestScreen(
                store: host.github, tree: tree, mac: PrMac(id: mac, name: host.name, offline: host.link != .online),
                newPr: NewPr(branch: head?.branch, base: head?.base, ahead: head?.ahead, commits: head?.commits ?? []),
                cleanup: cleanup(tree, changes: head?.totals.files),
                deleteTree: { try await host.workspace.deleteWorktree(path: tree, deleteBranch: false) },
                branches: { try await host.workspace.loadBranches(tree: tree) }
            )
            .onAppear { host.git.show(tree) }
            .onDisappear { host.git.hide(tree) }
        case .pullRequests(let mac, let project):
            PullRequestListScreen(store: host.github, mac: mac, macName: host.name, project: project)
        default:
            Placeholder("Pull request", systemImage: "arrow.triangle.pull")
        }
    }

    /// Set for a worktree other than the main one, when the phone may delete worktrees.
    private func cleanup(_ tree: String, changes: Int?) -> WorktreeCleanup? {
        guard host.workspace.canManage,
            let worktree = host.workspace.projects.lazy.flatMap(\.worktrees).first(where: { $0.path == tree && !$0.isMain })
        else { return nil }
        let cwds = host.sessions.sessions.values.map(\.summary.cwd)
        return WorktreeCleanup(name: worktree.name, changes: changes, sessions: WorktreeCleanup.sessions(in: tree, cwds: cwds))
    }
}
