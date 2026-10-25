import DesignSystem
import Domain
import Stores
import SwiftUI

/// One worktree's or Local's sessions, with what can be done to it. `key` is the worktree's path or the Local's id.
@MainActor
struct TreeView: View {
    let host: HostModel
    let mac: String
    let key: String
    @AppStorage("sessions.sort") private var sort = Sidebar.SessionSort.newest
    @State private var action: RowAction?
    @State private var making: Making?

    var body: some View {
        let built = host.homeList(sort: sort, open: key)
        let found = built.sections.lazy.compactMap { s in s.trees.first { $0.id == key }.map { (s, $0) } }.first
        Group {
            if let (section, tree) = found {
                List {
                    if tree.sessions.isEmpty {
                        Text("No sessions here yet.").foregroundStyle(Palette.text3.color)
                    }
                    ForEach(tree.sessions, id: \.self) { id in
                        if let session = host.sessions.session(id) {
                            NavigationLink(value: Route.chat(mac: mac, agentId: id)) { SessionRow(session: session) }
                                .modifier(SessionActions(host: host, session: session, action: $action))
                        }
                    }
                }
                .navigationTitle(tree.label)
                .toolbar {
                    ToolbarItem(placement: .primaryAction) {
                        Button { making = .session(project: section.id, place: tree.kind == .local ? .local(tree.id) : .tree(tree.id)) } label: {
                            Label("New Session", systemImage: "square.and.pencil")
                        }
                    }
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
        .sheet(item: $making) { making in MakingSheet(host: host, mac: mac, making: making) }
        .rowActions(host, $action)
    }
}
