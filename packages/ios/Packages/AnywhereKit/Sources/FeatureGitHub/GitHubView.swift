import ChatModel
import DesignSystem
import Domain
import Stores
import SwiftUI

/// A tree's pull request and a project's pull requests.
@MainActor
public struct GitHubView: View {
    let host: HostModel
    let route: Route
    let open: ((Route) -> Void)?
    /// The tree's status reads before this visit; deleting trusts only a change count read since.
    @State private var readsBefore: Int?

    /// `open` shows the chat a PR's problems were sent to, or a new session for them on a worktree without one.
    public init(host: HostModel, route: Route, open: ((Route) -> Void)? = nil) {
        self.host = host
        self.route = route
        self.open = open
    }

    public var body: some View {
        switch route {
        case .pullRequest(let mac, let tree, let chat):
            // The tree's status gives the new PR its commits and base, and its watch brings gh.changed.
            let changes = host.git.changes(tree)
            let head = changes.head
            PullRequestScreen(
                store: host.github, tree: tree, mac: PrMac(id: mac, name: host.name, offline: host.link != .online),
                newPr: NewPr(branch: head?.branch, base: head?.base, ahead: head?.ahead, commits: head?.commits ?? []),
                cleanup: cleanup(tree, changes: readsBefore.flatMap { changes.files(readSince: $0) }),
                deleteTree: { try await host.workspace.deleteWorktree(path: tree, deleteBranch: false) },
                branches: { try await host.workspace.loadBranches(tree: tree) },
                fix: fix(tree, mac: mac, chat: chat)
            )
            .onAppear {
                readsBefore = changes.reads
                host.git.show(tree, reread: true)
            }
            .onDisappear { host.git.hide(tree) }
        case .pullRequests(let mac, let project):
            PullRequestListScreen(store: host.github, mac: mac, macName: host.name, project: project)
        case .reviewThreads(let mac, let tree, let chat):
            ThreadsScreen(store: host.github, model: host.github.pullRequest(tree), mac: PrMac(id: mac, name: host.name, offline: host.link != .online),
                          fix: fix(tree, mac: mac, chat: chat))
        default:
            Placeholder("Pull request", systemImage: "arrow.triangle.pull")
        }
    }

    private func fix(_ tree: String, mac: String, chat: String?) -> FixContext {
        FixContext(
            mac: mac, opener: chat, project: host.workspace.place(of: tree)?.project,
            target: FixTarget(tree: tree, agents: host.sessions.sessions.values.map(\.summary), opener: chat),
            canDrive: !Composer.observeOnly(host.hello?.scopes ?? []),
            send: { host.chat($0).send(text: $1) },
            open: open)
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
