import DesignSystem
import Domain
import Stores
import SwiftUI

/// A rename, delete or close a row asked for, waiting on the user's word, or a new Local or pin, which need none.
enum RowAction: Identifiable, Hashable {
    case rename(HomeList.Tree)
    case delete(HomeList.Tree)
    case close(sessionId: String, title: String)
    case newLocal(project: String)
    case pin(sessionId: String, on: Bool)

    var id: String {
        switch self {
        case .rename(let t): "rename:" + t.id
        case .delete(let t): "delete:" + t.id
        case .close(let id, _): "close:" + id
        case .newLocal(let project): "local:" + project
        case .pin(let id, _): "pin:" + id
        }
    }
}

/// The asks behind row menus and swipes: a name for a rename, a confirm before deleting or closing, and what went wrong.
@MainActor
struct RowActions: ViewModifier {
    let host: HostModel
    @Binding var action: RowAction?
    @State private var name = ""
    @State private var failure: String?

    func body(content: Content) -> some View {
        content
            .alert("Rename", isPresented: shown { if case .rename = $0 { true } else { false } }, presenting: action) { action in
                TextField("Name", text: $name)
                Button("Cancel", role: .cancel) {}
                Button("Rename") { if case .rename(let tree) = action { rename(tree) } }
            }
            .confirmationDialog(title, isPresented: shown { if case .delete = $0 { true } else if case .close = $0 { true } else { false } },
                                titleVisibility: .visible,
                                presenting: action) { action in
                switch action {
                case .delete(let tree) where tree.kind == .worktree:
                    Button("Delete Worktree", role: .destructive) { delete(tree, branch: false) }
                    if let branch = tree.branch { Button("Delete Worktree and \(branch)", role: .destructive) { delete(tree, branch: true) } }
                case .delete(let tree):
                    Button("Delete \(tree.label)", role: .destructive) { delete(tree, branch: false) }
                case .close(let id, _):
                    Button("Close Session", role: .destructive) { close(id) }
                case .rename, .newLocal, .pin:
                    EmptyView()
                }
            } message: { action in
                switch action {
                case .delete(let tree) where tree.kind == .worktree: Text("Its terminals close and its folder is removed.")
                case .delete: Text("Its terminals close. The project's files stay.")
                case .close: Text("The agent and its terminal end.")
                case .rename, .newLocal, .pin: EmptyView()
                }
            }
            .alert("Something went wrong", isPresented: Binding(get: { failure != nil }, set: { if !$0 { failure = nil } })) {
                Button("OK", role: .cancel) {}
            } message: {
                Text(failure ?? "")
            }
            .onChange(of: action) { _, new in
                switch new {
                case .rename(let tree)?:
                    name = tree.label
                case .newLocal(let project)?:
                    action = nil
                    run { try await host.workspace.createLocal(project: project) }
                case .pin(let id, let on)?:
                    action = nil
                    run { try await host.sessions.pin(id, on) }
                default:
                    break
                }
            }
    }

    private var title: String {
        switch action {
        case .delete(let tree)?: "Delete \(tree.label)?"
        case .close(_, let title)?: "Close \(title)?"
        default: ""
        }
    }

    private func shown(_ matches: @escaping (RowAction) -> Bool) -> Binding<Bool> {
        Binding(get: { action.map(matches) ?? false }, set: { if !$0 { action = nil } })
    }

    private func rename(_ tree: HomeList.Tree) {
        let title = name
        run {
            if tree.kind == .local {
                try await host.workspace.renameLocal(id: tree.id, title: title)
            } else {
                try await host.workspace.renameWorktree(path: tree.id, title: title)
            }
        }
    }

    private func delete(_ tree: HomeList.Tree, branch: Bool) {
        run {
            if tree.kind == .local {
                try await host.workspace.deleteLocal(id: tree.id)
            } else {
                try await host.workspace.deleteWorktree(path: tree.id, deleteBranch: branch)
            }
            Haptics.success()
        }
    }

    private func close(_ id: String) {
        run { try await host.sessions.close(id) }
    }

    private func run(_ work: @escaping @MainActor () async throws -> Void) {
        Task {
            do { try await work() } catch {
                Haptics.warning()
                failure = WorkspaceStore.serverError(error).message
            }
        }
    }
}

extension View {
    @MainActor
    func rowActions(_ host: HostModel, _ action: Binding<RowAction?>) -> some View {
        modifier(RowActions(host: host, action: action))
    }
}

/// A session's swipes and menu: pin on the leading edge, close on the trailing one, both in its menu.
@MainActor
struct SessionActions: ViewModifier {
    let host: HostModel
    let session: SessionModel
    @Binding var action: RowAction?

    func body(content: Content) -> some View {
        let pinned = session.summary.pinned == true
        content
            .swipeActions(edge: .leading) {
                Button { pin(!pinned) } label: { Label(pinned ? "Unpin" : "Pin", systemImage: pinned ? "pin.slash" : "pin") }
                    .tint(Palette.accent.color)
            }
            .swipeActions(edge: .trailing) {
                // No destructive role: that slides the row out before the confirm.
                Button { close() } label: { Label("Close", systemImage: "xmark") }.tint(Palette.failed.color)
            }
            .contextMenu {
                Button { pin(!pinned) } label: { Label(pinned ? "Unpin" : "Pin", systemImage: pinned ? "pin.slash" : "pin") }
                Button(role: .destructive) { close() } label: { Label("Close Session", systemImage: "xmark") }
            }
    }

    private func pin(_ on: Bool) {
        Haptics.tap()
        action = .pin(sessionId: session.id, on: on)
    }

    private func close() {
        action = .close(sessionId: session.id, title: SessionText.title(session.summary))
    }
}
