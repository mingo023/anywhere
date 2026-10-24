import DesignSystem
import Domain
import Stores
import SwiftUI

/// Projects, worktrees and Locals with their sessions, Up next, new sessions and automations.
@MainActor
public struct SessionsView: View {
    let host: HostModel
    let route: Route

    public init(host: HostModel, route: Route) {
        self.host = host
        self.route = route
    }

    public var body: some View {
        Placeholder(host.name, systemImage: "rectangle.stack", detail: "Sessions")
            .navigationTitle(host.name)
    }
}
