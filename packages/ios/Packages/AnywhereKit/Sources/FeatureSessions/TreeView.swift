import DesignSystem
import Domain
import Stores
import SwiftUI
import Wire

/// One worktree's or Local's Changes, Files and pull request, then its sessions, with what can be done to it. `key` is
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
        let built = host.homeList(sort: sort, open: key)
        let found = built.sections.lazy.compactMap { s in s.trees.first { $0.id == key }.map { (s, $0) } }.first
        Group {
            if let (section, tree) = found {
                let creations = host.workspace.creations.filter { !$0.value.done && $0.value.place == key }.sorted { $0.key < $1.key }
                List {
                    Section {
                        TreeHeader(section: section, tree: tree, head: host.git.changes(checkout(tree, project: section.id)).head)
                    }
                    .listRowBackground(Color.clear)
                    .listRowInsets(EdgeInsets(top: 0, leading: Space.xs, bottom: Space.s, trailing: Space.xs))
                    .listRowSeparator(.hidden)
                    TreeLinks(host: host, mac: mac, project: section.id, tree: checkout(tree, project: section.id), key: key, open: open)
                    Section {
                        ForEach(creations, id: \.key) { id, creation in
                            CreationRow(creation: creation, retry: { host.workspace.runKept(id) }, dismiss: { host.workspace.forget(creation: id) })
                                .onTapGesture { making = .progress(requestId: id) }
                        }
                        if tree.sessions.isEmpty && creations.isEmpty {
                            Text("No sessions here yet.").foregroundStyle(Palette.text3.color)
                        }
                        ForEach(tree.sessions, id: \.self) { id in
                            if let session = host.sessions.session(id) {
                                PushRow(value: Route.chat(mac: mac, agentId: id)) { SessionRow(session: session) }
                                    .modifier(SessionActions(host: host, session: session, action: $action))
                                    .bareRow()
                            }
                        }
                    } header: {
                        HStack {
                            Text("Sessions").sectionTitle()
                            Spacer()
                            Text("\(tree.sessions.count)").font(Typography.footnote).foregroundStyle(Palette.text3.color)
                        }
                    }
                }
                .scrollContentBackground(.hidden)
                .screenBackground()
                .navigationTitle(tree.label)
                .toolbar {
                    NoBarTitle()
                    if host.workspace.canManage {
                        ToolbarItem(placement: .primaryAction) {
                            Menu {
                                Button { action = .rename(tree) } label: { Label("Rename", systemImage: "pencil") }
                                if tree.kind != .own {
                                    Button(role: .destructive) { action = .delete(tree) } label: { Label("Delete", systemImage: "trash") }
                                }
                            } label: { Label("More", systemImage: "ellipsis") }
                        }
                    }
                    #if os(iOS) && compiler(>=6.2)
                        if #available(iOS 26, *) { ToolbarSpacer(.fixed, placement: .primaryAction) }
                    #endif
                    ToolbarItem(placement: .primaryAction) { newSession(section, tree) }
                }
                #if os(iOS)
                    .navigationBarTitleDisplayMode(.inline)
                #endif
            } else {
                Placeholder("It's gone", systemImage: "folder.badge.questionmark", detail: "This worktree or Local is no longer on the Mac.")
            }
        }
        .stale(host)
        .sheet(item: $making) { making in MakingSheet(host: host, mac: mac, making: making) }
        /* Back to the list first: the tree is gone from it at once, and the list says how the delete went. On iPad this
           screen is the column's root, which only the navigator can leave. */
        .rowActions(host, $action, deleting: { if let leave { leave(key) } else { dismiss() } })
    }

    private func newSession(_ section: HomeList.Section, _ tree: HomeList.Tree) -> some View {
        Button { making = .session(project: section.id, place: TreeMenu.place(tree)) } label: {
            Label("New Session", systemImage: "plus")
        }
    }

    /// The folder its git and files are read in: a Local's is its project's own checkout.
    private func checkout(_ tree: HomeList.Tree, project: String) -> String {
        guard tree.kind == .local else { return tree.id }
        return NewSessionView.main(of: host.workspace.project(project)) ?? project
    }
}

/// A worktree's or Local's title block: what kind of place it is, its name, and its branch against its base.
@MainActor
struct TreeHeader: View {
    let section: HomeList.Section
    let tree: HomeList.Tree
    let head: GitHead?

    var body: some View {
        VStack(alignment: .leading, spacing: Space.xs) {
            Text("\(section.name) · \(tree.kind == .worktree ? "worktree" : "Local")").font(Typography.footnote).foregroundStyle(Palette.text3.color)
            Text(tree.label).font(Typography.largeTitle).foregroundStyle(Palette.text.color).lineLimit(1).minimumScaleFactor(0.7)
            if let line = Self.branchLine(branch: tree.branch, head: head) {
                Text(line).font(Typography.codeSmall).foregroundStyle(Palette.text3.color).lineLimit(1).truncationMode(.middle)
            }
        }
    }

    /// "fix/ws-reconnect from main · 3 ahead".
    nonisolated static func branchLine(branch: String?, head: GitHead?) -> String? {
        guard let branch else { return nil }
        guard let base = head?.base else { return branch }
        return "\(branch) from \(base) · \(head?.ahead ?? 0) ahead"
    }
}
