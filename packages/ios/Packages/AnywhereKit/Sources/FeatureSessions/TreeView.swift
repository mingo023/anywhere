import DesignSystem
import Domain
import Stores
import SwiftUI
import Wire

/// One worktree's or Local's Changes, pull request and Files, then its sessions, with what can be done to it. `key` is
/// the worktree's path or the Local's id.
@MainActor
struct TreeView: View {
    let host: HostModel
    let mac: String
    let key: String
    var open: ((Route) -> Void)?
    var leave: ((String) -> Void)?
    @AppStorage("sessions.sort") private var sort = Sidebar.SessionSort.newest
    @State private var action: RowAction?
    @State private var making: Making?
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        TimelineView(.explicit(host.reachabilityMarks)) { _ in
            screen(asOf: host.staleAsOf(at: .now))
        }
        .sheet(item: $making) { making in MakingSheet(host: host, mac: mac, making: making) }
        /* Back to the list first: the tree is gone from it at once, and the list says how the delete went. On iPad this
           screen is the column's root, which only the navigator can leave. */
        .rowActions(host, $action, deleting: { if let leave { leave(key) } else { dismiss() } })
    }

    @ViewBuilder
    private func screen(asOf: Int?) -> some View {
        let built = host.homeList(sort: sort, open: key)
        let found = built.sections.lazy.compactMap { s in s.trees.first { $0.id == key }.map { (s, $0) } }.first
        Group {
            if let (section, tree) = found {
                let checkout = checkout(tree, project: section.id)
                let creations = host.workspace.creations.filter { !$0.value.done && $0.value.place == key }.sorted { $0.key < $1.key }
                ScrollView {
                    VStack(alignment: .leading, spacing: 0) {
                        VStack(alignment: .leading, spacing: 0) {
                            header(section, tree, head: host.git.changes(checkout).head, stale: asOf != nil)
                            TreeLinks(host: host, mac: mac, project: section.id, tree: checkout).padding(.top, Space.xl)
                            GroupHeader("Sessions")
                            sessions(tree, creations: creations)
                        }
                        .opacity(asOf == nil ? 1 : Staleness.dim)
                        if let asOf {
                            Text(TreeText.offlineNote(asOf: asOf)).font(.system(size: 13)).foregroundStyle(Palette.text3.color)
                                .padding(.horizontal, Inset.margin * 2).padding(.top, Space.s)
                        }
                    }
                    .padding(.bottom, Space.xl)
                }
                .cornerButtons(back: true) { newSession(section, tree, stale: asOf != nil) }
            } else {
                Placeholder("It's gone", icon: .folder, detail: "This worktree or Local is no longer on the Mac.")
            }
        }
        .environment(\.staleSince, asOf)
        .screenBackground()
    }

    private func header(_ section: HomeList.Section, _ tree: HomeList.Tree, head: GitHead?, stale: Bool) -> some View {
        VStack(alignment: .leading, spacing: 0) {
            Text(stale ? "\(section.name) · \(host.name)" : section.name).font(.system(size: 13)).foregroundStyle(Palette.text2.color)
            HStack(spacing: Space.s) {
                Text(tree.label).font(.system(size: 28, weight: .bold)).foregroundStyle(Palette.text.color).lineLimit(2)
                    .frame(maxWidth: .infinity, alignment: .leading)
                if host.workspace.canManage && !stale { TitleMenu { menu(tree) } }
            }
            .padding(.top, Space.xxs)
            if let line = TreeText.branchLine(branch: tree.branch ?? tree.place, label: tree.label, head: head) {
                Text(line).font(.system(size: 13, design: .monospaced)).foregroundStyle(Palette.text2.color).lineLimit(1).truncationMode(.middle)
                    .padding(.top, Space.xs)
            }
        }
        .padding(.horizontal, 20)
        .padding(.top, stale ? 18 : 6)
    }

    @ViewBuilder
    private func menu(_ tree: HomeList.Tree) -> some View {
        Button { action = .rename(tree) } label: { Label("Rename", lucide: .pencil) }
        if tree.kind != .own {
            Button(role: .destructive) { action = .delete(tree) } label: { Label("Delete", lucide: .trash) }
        }
    }

    private func sessions(_ tree: HomeList.Tree, creations: [(key: String, value: Creation)]) -> some View {
        VStack(spacing: 0) {
            ForEach(Array(creations.enumerated()), id: \.element.key) { index, creation in
                CreationRow(creation: creation.value, retry: { host.workspace.runKept(creation.key) }, dismiss: { host.workspace.forget(creation: creation.key) })
                    .onTapGesture { making = .progress(requestId: creation.key) }
                    .rowDivider(index > 0)
            }
            if tree.sessions.isEmpty && creations.isEmpty {
                Text("No sessions here yet.").font(.system(size: 15)).foregroundStyle(Palette.text2.color)
                    .padding(.vertical, 28).frame(maxWidth: .infinity)
            }
            ForEach(Array(tree.sessions.enumerated()), id: \.element) { index, id in
                if let session = host.sessions.session(id) {
                    PushRow(value: Route.chat(mac: mac, agentId: id)) { SessionRow(session: session) }
                        .buttonStyle(.plain)
                        .modifier(SessionActions(host: host, session: session, action: $action))
                        .rowDivider(index > 0 || !creations.isEmpty)
                }
            }
        }
        .insetGroup()
    }

    private func newSession(_ section: HomeList.Section, _ tree: HomeList.Tree, stale: Bool) -> some View {
        Button { making = .session(project: section.id, place: TreeMenu.place(tree)) } label: {
            LucideIcon(.plus, size: 20).foregroundStyle(Palette.text.color)
        }
        .disabled(stale || host.link != .online)
        .accessibilityLabel("New session")
    }

    /// The folder its git and files are read in: a Local's is its project's own checkout.
    private func checkout(_ tree: HomeList.Tree, project: String) -> String {
        guard tree.kind == .local else { return tree.id }
        return host.workspace.project(project).map { $0.worktrees.first(where: \.isMain)?.path ?? $0.path } ?? project
    }
}

extension TreeText {
    /// "fix/ws-reconnect from main · 3 ahead", or "from main · 3 ahead" where the branch is the title already.
    static func branchLine(branch: String?, label: String?, head: GitHead?) -> String? {
        guard let branch else { return nil }
        let name = branch == label ? nil : branch
        let against = head?.base.map { "from \($0) · \(head?.ahead ?? 0) ahead" }
        let line = [name, against].compactMap(\.self).joined(separator: " ")
        return line.isEmpty ? nil : line
    }

    /// Under an offline Mac's tree: when its data is from, and what is off.
    static func offlineNote(asOf: Int, timeZone: TimeZone = .current) -> String {
        "Offline · as of \(TimeText.clock(ms: asOf, timeZone: timeZone)). New sessions and changes are off."
    }
}

enum Staleness {
    /// How far a Mac's last-known data fades while the Mac is away.
    static let dim = 0.45
}
