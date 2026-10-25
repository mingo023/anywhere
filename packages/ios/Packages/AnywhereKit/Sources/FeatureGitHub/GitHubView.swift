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
        case .pullRequest(_, let tree):
            // The tree's status gives the new PR its commits and base, and its watch brings gh.changed.
            let head = host.git.changes(tree).head
            PullRequestScreen(
                store: host.github, tree: tree, commits: head?.commits ?? [], base: head?.base.map(PullRequestText.baseBranch),
                offersCreate: PullRequestText.offersCreate(branch: head?.branch, base: head?.base)
            )
            .onAppear { host.git.show(tree) }
            .onDisappear { host.git.hide(tree) }
        case .pullRequests(let mac, let project):
            PullRequestListScreen(store: host.github, mac: mac, project: project)
        default:
            Placeholder("Pull request", systemImage: "arrow.triangle.pull")
        }
    }
}
