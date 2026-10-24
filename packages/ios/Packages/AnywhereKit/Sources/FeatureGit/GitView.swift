import DesignSystem
import Domain
import Stores
import SwiftUI
import Wire

/// A tree's changes, a diff, the commit graph, a commit and branches.
@MainActor
public struct GitView: View {
    let host: HostModel
    let route: Route

    public init(host: HostModel, route: Route) {
        self.host = host
        self.route = route
    }

    public var body: some View {
        switch route {
        case .changes(let mac, let tree):
            ChangesScreen(host: host, mac: mac, tree: tree)
        case .diff(_, let tree, let path, let staged):
            let files = host.git.changes(tree).files.filter { staged ? $0.staged : $0.unstaged }.map(\.path)
            DiffScreen(host: host, tree: tree, path: path, at: staged ? .staged : .working, siblings: files)
        case .history(let mac, let tree):
            HistoryScreen(host: host, mac: mac, tree: tree)
        case .commit(_, let tree, let sha):
            CommitScreen(host: host, tree: tree, sha: sha)
        case .branches:
            Placeholder("Branches", systemImage: "arrow.triangle.branch", detail: "Pick a branch when you make a worktree.")
        default:
            Placeholder("Changes", systemImage: "arrow.triangle.branch")
        }
    }
}
