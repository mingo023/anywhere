import DesignSystem
import Domain
import Stores
import SwiftUI

/// A tree's files and a file's preview.
@MainActor
public struct FilesView: View {
    let host: HostModel
    let route: Route
    let open: ((Route) -> Void)?

    /// `open` shows another feature's screen: the chat a mention went to, or a new session for lines on a tree without
    /// one. It should go back to a chat already on the stack rather than push it again.
    public init(host: HostModel, route: Route, open: ((Route) -> Void)? = nil) {
        self.host = host
        self.route = route
        self.open = open
    }

    public var body: some View {
        switch route {
        case .files(_, let tree, let dir):
            FileTreeScreen(files: host.files, model: host.files.tree(tree), dir: dir, context: context(tree))
        case .file(_, let tree, let path, let line):
            if let path = FileText.relative(path, tree: tree) {
                FilePreviewScreen(files: host.files, preview: host.files.preview(tree: tree, path: path), line: line, context: context(tree))
            } else {
                Placeholder("This file is outside \(FileText.baseName(tree))", systemImage: "folder.badge.questionmark", detail: path)
            }
        default:
            Placeholder("Files", systemImage: "folder")
        }
    }

    private func context(_ tree: String) -> FilesContext {
        FilesContext(
            mac: route.mac ?? host.mac,
            target: ChatTarget(ChatSession.on(tree, host.sessions.sessions.values.map(\.summary)), open: Set(host.chats.keys)),
            project: host.workspace.projects.first { $0.path == tree || $0.worktrees.contains { $0.path == tree } }?.path,
            open: open,
            git: host.git)
    }
}
