import DesignSystem
import Domain
import Stores
import SwiftUI

/// A tree's files and a file's preview.
@MainActor
public struct FilesView: View {
    let host: HostModel
    let route: Route

    public init(host: HostModel, route: Route) {
        self.host = host
        self.route = route
    }

    public var body: some View {
        switch route {
        case .files(_, let tree, let dir):
            FileTreeScreen(files: host.files, model: host.files.tree(tree), dir: dir)
        case .file(_, let tree, let path):
            FilePreviewScreen(files: host.files, preview: host.files.preview(tree: tree, path: path))
        default:
            Placeholder("Files", systemImage: "folder")
        }
    }
}
