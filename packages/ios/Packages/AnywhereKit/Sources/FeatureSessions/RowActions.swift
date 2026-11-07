import DesignSystem
import Domain
import Stores
import SwiftUI

/// A rename, delete or close a row asked for, waiting on the user's word, or a new Local or pin, which need none.
enum RowAction: Identifiable, Hashable {
    case rename(HomeList.Tree)
    case delete(HomeList.Tree)
    /// Deleting the worktree's branch was refused: `removal.unpushed` commits on it are nowhere else.
    case forceDelete(Removal)
    case close(sessionId: String, title: String)
    case newLocal(project: String)
    case pin(sessionId: String, on: Bool)

    var id: String {
        switch self {
        case .rename(let t): "rename:" + t.id
        case .delete(let t): "delete:" + t.id
        case .forceDelete(let r): "force:" + r.id
        case .close(let id, _): "close:" + id
        case .newLocal(let project): "local:" + project
        case .pin(let id, _): "pin:" + id
        }
    }

    /// Asks for a word in the confirm dialog.
    var confirms: Bool {
        switch self {
        case .delete, .forceDelete, .close: true
        case .rename, .newLocal, .pin: false
        }
    }
}

/// The asks behind row menus and swipes: a name for a rename, a confirm before deleting or closing, and what went wrong.
/// While its list is on screen it also says what happened out of sight: a worktree or Local being deleted, or failing
/// to be, and a session that started after its sheet was hidden. `deleting` runs before a delete goes out, so a screen
/// of the tree being deleted can leave first.
@MainActor
struct RowActions: ViewModifier {
    let host: HostModel
    @Binding var action: RowAction?
    var deleting: (() -> Void)?
    @State private var name = ""
    @State private var failure: String?
    @State private var toast: Toast?
    @State private var visible = false
    @Environment(\.openURL) private var openURL

    func body(content: Content) -> some View {
        content
            .alert("Rename", isPresented: shown { if case .rename = $0 { true } else { false } }, presenting: action) { action in
                TextField("Name", text: $name)
                Button("Cancel", role: .cancel) {}
                Button("Rename") { if case .rename(let tree) = action { rename(tree) } }
            }
            .confirmationDialog(title, isPresented: shown(\.confirms), titleVisibility: .visible, presenting: action) { action in
                switch action {
                case .delete(let tree) where tree.kind == .worktree:
                    Button("Delete Worktree", role: .destructive) { delete(tree, branch: false) }
                    if let branch = tree.branch { Button("Delete Worktree and \(branch)", role: .destructive) { delete(tree, branch: true) } }
                case .delete(let tree):
                    Button("Delete \(tree.label)", role: .destructive) { delete(tree, branch: false) }
                case .forceDelete(let removal):
                    Button("Delete Anyway", role: .destructive) {
                        var forced = removal
                        forced.force = true
                        remove(forced)
                    }
                case .close(let id, _):
                    Button("Close Session", role: .destructive) { close(id) }
                case .rename, .newLocal, .pin:
                    EmptyView()
                }
            } message: { action in
                switch action {
                case .delete(let tree) where tree.kind == .worktree: Text("Its terminals close and its folder is removed.")
                case .delete: Text("Its terminals close. The project's files stay.")
                case .forceDelete(let removal): Text(Self.lostMessage(removal.unpushed ?? 0))
                case .close: Text("The agent and its terminal end.")
                case .rename, .newLocal, .pin: EmptyView()
                }
            }
            .alert("Something went wrong", isPresented: Binding(get: { failure != nil }, set: { if !$0 { failure = nil } })) {
                Button("OK", role: .cancel) {}
            } message: {
                Text(failure ?? "")
            }
            .toast($toast)
            .onAppear {
                visible = true
                notice()
            }
            .onDisappear { visible = false }
            .onChange(of: host.workspace.removals) { notice() }
            .onChange(of: host.workspace.creations) { notice() }
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
        case .forceDelete(let removal)?: Self.lostTitle(removal.unpushed ?? 0, branch: removal.branch ?? removal.label)
        case .close(_, let title)?: "Close \(title)?"
        default: ""
        }
    }

    nonisolated static func lostTitle(_ count: Int, branch: String) -> String {
        "Delete \(branch) and its \(count) unpushed commit\(count == 1 ? "" : "s")?"
    }

    nonisolated static func lostMessage(_ count: Int) -> String {
        count == 1
            ? "That commit is on no other branch, remote or tag, so it will be lost."
            : "Those \(count) commits are on no other branch, remote or tag, so they will be lost."
    }

    private func shown(_ matches: @escaping (RowAction) -> Bool) -> Binding<Bool> {
        Binding(get: { action.map(matches) ?? false }, set: { if !$0 { action = nil } })
    }

    /// Says what happened while this list was out of sight, or since: one list says it, the one on screen.
    private func notice() {
        guard visible else { return }
        let workspace = host.workspace
        if let failed = workspace.takeFailedRemoval() {
            Haptics.warning()
            if failed.unpushed != nil {
                toast = nil
                action = .forceDelete(failed)
            } else {
                toast = Toast(failed.error ?? "", lasts: .seconds(6), actionTitle: "Try Again") { remove(failed) }
            }
        } else if let going = workspace.removals.values.first(where: { !$0.failed }) {
            let text = "Deleting \(going.label)…"
            if toast?.text != text { toast = Toast(text, lasts: nil) }
        } else if toast?.lasts == nil {
            toast = nil
        }
        for started in workspace.takeStarted() {
            guard let agentId = started.agentId else { continue }
            Haptics.success()
            let mac = host.mac
            toast = Toast("Session started", actionTitle: "Open") { openURL(DeepLink.agent(mac: mac, agentId: agentId).url) }
        }
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
        deleting?()
        remove(Removal(id: tree.id, label: tree.label, local: tree.kind == .local, branch: tree.branch, deleteBranch: branch))
    }

    private func remove(_ removal: Removal) {
        let workspace = host.workspace
        Task {
            await workspace.remove(removal)
            if workspace.removals[removal.id] == nil { Haptics.success() }
        }
    }

    private func close(_ id: String) {
        run { try await host.sessions.close(id) }
    }

    private func run(_ work: @escaping @MainActor () async throws -> Void) {
        Task {
            do { try await work() } catch {
                guard let text = ErrorText.describe(error, mac: host.name) else { return }
                Haptics.warning()
                failure = text
            }
        }
    }
}

extension View {
    @MainActor
    func rowActions(_ host: HostModel, _ action: Binding<RowAction?>, deleting: (() -> Void)? = nil) -> some View {
        modifier(RowActions(host: host, action: action, deleting: deleting))
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
