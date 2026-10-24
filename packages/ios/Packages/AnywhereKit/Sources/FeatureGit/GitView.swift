import DesignSystem
import Domain
import Stores
import SwiftUI

/// A tree's changes, a diff, the commit graph, a commit and branches.
@MainActor
public struct GitView: View {
    let host: HostModel
    let route: Route

    public init(host: HostModel, route: Route) {
        self.host = host
        self.route = route
    }

    public var body: some View {
        Placeholder("Changes", systemImage: "arrow.triangle.branch")
    }
}
