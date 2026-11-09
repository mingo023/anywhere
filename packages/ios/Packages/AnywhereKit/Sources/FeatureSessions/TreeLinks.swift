import DesignSystem
import Domain
import Stores
import SwiftUI
import Wire

/// A worktree's or Local's ways into its code, as four tiles: Changes, its pull request, Files and Terminals. `tree` is
/// the checkout they read: the worktree, or a Local's project checkout. `key` is the worktree's path or the Local's id.
@MainActor
struct TreeLinks: View {
    let host: HostModel
    let mac: String
    let project: String
    let tree: String
    let key: String
    let open: ((Route) -> Void)?

    var body: some View {
        let head = host.git.changes(tree).head
        let item = host.github.lists[project]?.first { $0.tree == tree }
        let pr = Self.pullRequest(item, gh: host.github.status?.state, problem: host.github.pullRequest(tree).problem, readsGitHub: host.hello?.has(Caps.gh) == true)
        let terminals = host.terminals.terminals.filter { $0.localId == key || ($0.localId == nil && $0.cwd == tree) }
        let busy = terminals.count { $0.agentId.flatMap { host.sessions.session($0)?.status } == .working }
        Section {
            LazyVGrid(columns: [GridItem(.flexible(), spacing: Space.s), GridItem(.flexible())], spacing: Space.s) {
                TileLink("Changes", value: Route.changes(mac: mac, tree: tree), open: open) {
                    if let totals = head?.totals, totals.files > 0 {
                        (Text(Self.changes(totals.files) ?? "") + Text(" +\(totals.added)").foregroundColor(Palette.diffAddText.color)
                            + Text(" −\(totals.removed)").foregroundColor(Palette.diffDelText.color))
                    } else {
                        Text(Self.changes(head?.totals.files) ?? " ")
                    }
                }
                if let pr {
                    TileLink(pr.capitalized, trailing: item.map { "#\($0.number)" }, value: Route.pullRequest(mac: mac, tree: tree), open: open) {
                        if let item {
                            HStack(spacing: Space.xs) {
                                Circle().fill(Pill.colors(PrBadge.tone(item)).text.color).frame(width: 6, height: 6)
                                Text(SessionText.pullRequestState(item))
                            }
                        } else {
                            // Keeps the tile as tall as those with a line here.
                            Text(" ")
                        }
                    }
                }
                TileLink("Files", value: Route.files(mac: mac, tree: tree, dir: ""), open: open) {
                    Text(FileText.home(tree)).truncationMode(.head)
                }
                TileLink("Terminals", value: Route.terminals(mac: mac), open: open) {
                    HStack(spacing: Space.xs) {
                        if busy > 0 { GradientSpinner(size: 10) }
                        Text(Self.terminals(open: terminals.count, busy: busy))
                    }
                }
            }
        }
        .listRowBackground(Color.clear)
        .listRowInsets(EdgeInsets(top: Space.xs, leading: 0, bottom: Space.s, trailing: 0))
        .listRowSeparator(.hidden)
        .onAppear { host.git.show(tree) }
        .onDisappear { host.git.hide(tree) }
        .task {
            guard host.hello?.has(Caps.gh) == true else { return }
            // Only the tree's own answer says it has no GitHub remote.
            await host.github.refresh(tree)
            try? await host.github.refreshList(project)
        }
    }

    /// "No changes", "1 file", "12 files"; nil before the status loads.
    nonisolated static func changes(_ files: Int?) -> String? {
        files.map { $0 == 0 ? "No changes" : $0 == 1 ? "1 file" : "\($0) files" }
    }

    /// "2 open · 1 busy"; "None open" without any.
    nonisolated static func terminals(open: Int, busy: Int) -> String {
        if open == 0 { return "None open" }
        return busy > 0 ? "\(open) open · \(busy) busy" : "\(open) open"
    }

    /// The pull request tile's title: "Create pull request" once gh can make one. Nil hides the tile, for a repository with
    /// no GitHub remote or a Mac that can't read GitHub.
    nonisolated static func pullRequest(_ item: PrItem?, gh: GhState?, problem: GhState?, readsGitHub: Bool) -> String? {
        if !readsGitHub || problem == .noRemote { return nil }
        return item == nil && gh == .ready ? "Create pull request" : "Pull request"
    }
}

/// One way into a tree: a title, an optional number at its end, and a line of mono detail.
@MainActor
private struct TileLink<Detail: View>: View {
    let title: String
    var trailing: String?
    let value: Route
    let open: ((Route) -> Void)?
    @ViewBuilder let detail: Detail

    init(_ title: String, trailing: String? = nil, value: Route, open: ((Route) -> Void)?, @ViewBuilder detail: () -> Detail) {
        self.title = title
        self.trailing = trailing
        self.value = value
        self.open = open
        self.detail = detail()
    }

    var body: some View {
        // A button, not a link: a List row takes any tap for the last of the links it holds, whatever their style.
        Button { open?(value) } label: {
            VStack(alignment: .leading, spacing: Space.xs) {
                HStack {
                    Text(title).font(Typography.subheadline.weight(.semibold)).foregroundStyle(Palette.text.color).lineLimit(1)
                    Spacer(minLength: Space.xs)
                    if let trailing { Text(trailing).font(Typography.digits).foregroundStyle(Palette.text3.color) }
                }
                detail.font(Typography.codeSmall).foregroundStyle(Palette.text3.color).lineLimit(1)
            }
            .padding(Space.m)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(Palette.surface.color, in: RoundedRectangle(cornerRadius: Radius.card, style: .continuous))
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
    }
}
