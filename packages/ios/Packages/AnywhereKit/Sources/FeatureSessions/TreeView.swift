import DesignSystem
import Domain
import Stores
import SwiftUI

/// One worktree's or Local's Changes, Files and pull request, then its sessions, with what can be done to it. `key` is
/// the worktree's path or the Local's id.
@MainActor
struct TreeView: View {
    let host: HostModel
    let mac: String
    let key: String
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
                    TreeLinks(host: host, mac: mac, project: section.id, tree: checkout(tree, project: section.id))
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
                                NavigationLink(value: Route.chat(mac: mac, agentId: id)) { SessionRow(session: session) }
                                    .modifier(SessionActions(host: host, session: session, action: $action))
                            }
                        }
                    } header: {
                        Text("Sessions")
                    }
                }
                .navigationTitle(tree.label)
                .toolbar {
                    #if os(iOS)
                        ToolbarItemGroup(placement: .bottomBar) {
                            Spacer()
                            newSession(section, tree)
                        }
                    #else
                        ToolbarItem(placement: .primaryAction) { newSession(section, tree) }
                    #endif
                    if host.workspace.canManage {
                        ToolbarItem(placement: .secondaryAction) {
                            Menu {
                                Button { action = .rename(tree) } label: { Label("Rename", systemImage: "pencil") }
                                if tree.kind != .own {
                                    Button(role: .destructive) { action = .delete(tree) } label: { Label("Delete", systemImage: "trash") }
                                }
                            } label: { Label("More", systemImage: "ellipsis.circle") }
                        }
                    }
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
        // Back to the list first: the tree is gone from it at once, and the list says how the delete went. On iPad this
        // screen is the column's root, which only the navigator can leave.
        .rowActions(host, $action, deleting: { if let leave { leave(key) } else { dismiss() } })
    }

    private func newSession(_ section: HomeList.Section, _ tree: HomeList.Tree) -> some View {
        Button { making = .session(project: section.id, place: TreeMenu.place(tree)) } label: {
            Label("New Session", systemImage: "square.and.pencil")
        }
    }

    /// The folder its git and files are read in: a Local's is its project's own checkout.
    private func checkout(_ tree: HomeList.Tree, project: String) -> String {
        guard tree.kind == .local else { return tree.id }
        return NewSessionView.main(of: host.workspace.project(project)) ?? project
    }
}
