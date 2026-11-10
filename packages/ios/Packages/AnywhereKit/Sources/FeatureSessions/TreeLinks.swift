import DesignSystem
import Domain
import Stores
import SwiftUI
import Wire

/// A worktree's or Local's ways into its code, as tiles: Changes, its pull request and Files. `tree` is the checkout
/// they read: the worktree, or a Local's project checkout.
@MainActor
struct TreeLinks: View {
    let host: HostModel
    let mac: String
    let project: String
    let tree: String
    let open: ((Route) -> Void)?
    @Environment(\.showInDetail) private var showInDetail

    var body: some View {
        let column = showInDetail != nil
        let head = host.git.changes(tree).head
        let item = host.github.lists[project]?.first { $0.tree == tree }
        let pr = Self.pullRequest(item, gh: host.github.status?.state, problem: host.github.pullRequest(tree).problem, readsGitHub: host.hello?.has(Caps.gh) == true)
        Section {
            LazyVGrid(columns: column ? [GridItem(.flexible())] : [GridItem(.flexible(), spacing: Space.s), GridItem(.flexible())], spacing: column ? 0 : Space.s) {
                TileLink("Changes", systemImage: "plus.forwardslash.minus", first: true, value: Route.changes(mac: mac, tree: tree), open: open) {
                    if let totals = head?.totals, totals.files > 0 {
                        (Text(Self.changes(totals.files) ?? "") + Text(" +\(totals.added)").foregroundColor(Palette.diffAddText.color)
                            + Text(" −\(totals.removed)").foregroundColor(Palette.diffDelText.color))
                    } else {
                        Text(Self.changes(head?.totals.files) ?? " ")
                    }
                }
                if let pr {
                    TileLink(pr.capitalized, systemImage: "arrow.triangle.pull", trailing: item.map { "#\($0.number)" }, value: Route.pullRequest(mac: mac, tree: tree), open: open) {
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
                TileLink("Files", systemImage: "folder", value: Route.files(mac: mac, tree: tree, dir: ""), open: open) {
                    Text(FileText.home(tree)).truncationMode(.head)
                }
            }
            .background(column ? Palette.surface.color : .clear, in: RoundedRectangle(cornerRadius: Radius.card, style: .continuous))
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

    /// The pull request tile's title: "Create pull request" once gh can make one. Nil hides the tile, for a repository with
    /// no GitHub remote or a Mac that can't read GitHub.
    nonisolated static func pullRequest(_ item: PrItem?, gh: GhState?, problem: GhState?, readsGitHub: Bool) -> String? {
        if !readsGitHub || problem == .noRemote { return nil }
        return item == nil && gh == .ready ? "Create pull request" : "Pull request"
    }
}

/// One way into a tree: a tile of a title, an optional number at its end and a line of mono detail, or, in an iPad column,
/// where height is short, one line of a card with its icon.
@MainActor
private struct TileLink<Detail: View>: View {
    let title: String
    let systemImage: String
    var first = false
    var trailing: String?
    let value: Route
    let open: ((Route) -> Void)?
    @ViewBuilder let detail: Detail
    @Environment(\.showInDetail) private var showInDetail

    init(_ title: String, systemImage: String, first: Bool = false, trailing: String? = nil, value: Route, open: ((Route) -> Void)?, @ViewBuilder detail: () -> Detail) {
        self.title = title
        self.systemImage = systemImage
        self.first = first
        self.trailing = trailing
        self.value = value
        self.open = open
        self.detail = detail()
    }

    var body: some View {
        // A button, not a link: a List row takes any tap for the last of the links it holds, whatever their style.
        Button { open?(value) } label: {
            if showInDetail != nil { row } else { tile }
        }
        .buttonStyle(.plain)
    }

    private var tile: some View {
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

    private var row: some View {
        HStack(spacing: Space.m) {
            Image(systemName: systemImage).font(.system(size: 16)).foregroundStyle(Palette.text2.color).frame(width: 16)
            Text(title).font(Typography.subheadline).foregroundStyle(Palette.text.color).lineLimit(1).layoutPriority(1)
            Spacer(minLength: Space.xs)
            if let trailing { Text(trailing).font(Typography.digits).foregroundStyle(Palette.text3.color) }
            detail.font(Typography.codeSmall).foregroundStyle(Palette.text3.color).lineLimit(1)
            SidebarChevron(open: false)
        }
        .padding(.horizontal, Space.m)
        .frame(minHeight: HitTarget.minimum)
        .overlay(alignment: .top) {
            if !first { Palette.separator.color.frame(height: 1).padding(.leading, Space.m * 2 + 16) }
        }
        .contentShape(Rectangle())
    }
}
