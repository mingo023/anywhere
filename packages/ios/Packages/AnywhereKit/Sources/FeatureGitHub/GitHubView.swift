import DesignSystem
import Domain
import Stores
import SwiftUI

/// A tree's pull request and a project's pull requests.
@MainActor
public struct GitHubView: View {
    let host: HostModel
    let route: Route

    public init(host: HostModel, route: Route) {
        self.host = host
        self.route = route
    }

    public var body: some View {
        Placeholder("Pull request", systemImage: "arrow.triangle.pull")
    }
}
