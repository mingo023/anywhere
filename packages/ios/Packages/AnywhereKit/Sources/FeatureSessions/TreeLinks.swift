import DesignSystem
import Domain
import Stores
import SwiftUI
import Wire

/// A worktree's or Local's ways into its code: Changes with how many files changed, Files, and its pull request. `tree`
/// is the checkout they read: the worktree, or a Local's project checkout.
@MainActor
struct TreeLinks: View {
    let host: HostModel
    let mac: String
    let project: String
    let tree: String

    var body: some View {
        let changes = host.git.changes(tree)
        let pr = Self.pullRequest(host.github.lists[project]?.first { $0.tree == tree }, gh: host.github.status?.state,
                                  problem: host.github.pullRequest(tree).problem)
        Section {
            NavigationLink(value: Route.changes(mac: mac, tree: tree)) {
                LabeledContent {
                    if let count = Self.changes(changes.head?.totals.files) { Text(count) }
                } label: {
                    Label("Changes", systemImage: "plusminus")
                }
            }
            NavigationLink(value: Route.files(mac: mac, tree: tree, dir: "")) { Label("Files", systemImage: "folder") }
            if let pr {
                NavigationLink(value: Route.pullRequest(mac: mac, tree: tree)) {
                    LabeledContent {
                        if let detail = pr.detail { Text(detail) }
                    } label: {
                        Label(pr.title, systemImage: "arrow.triangle.pull")
                    }
                }
            }
        }
        .onAppear { host.git.show(tree) }
        .onDisappear { host.git.hide(tree) }
        .task {
            guard host.hello?.has(Caps.gh) == true else { return }
            try? await host.github.refreshList(project)
        }
    }

    /// "No changes", "1 file", "12 files"; nil before the status loads.
    nonisolated static func changes(_ files: Int?) -> String? {
        files.map { $0 == 0 ? "No changes" : $0 == 1 ? "1 file" : "\($0) files" }
    }

    /// The pull request row: the PR's number and checks, else "Create pull request" once gh can make one. Nil hides the
    /// row, for a repository with no GitHub remote.
    nonisolated static func pullRequest(_ item: PrItem?, gh: GhState?, problem: GhState?) -> (title: String, detail: String?)? {
        if problem == .noRemote { return nil }
        if let item { return ("Pull request", SessionText.pullRequest(item)) }
        return (gh == .ready ? "Create pull request" : "Pull request", nil)
    }
}
