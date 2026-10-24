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
        Placeholder("Files", systemImage: "folder")
    }
}
