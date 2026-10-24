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
        let mac = route.mac ?? host.hello?.serverId ?? host.id
        switch route {
        case .project(_, let path):
            HomeView(host: host, mac: mac, only: path)
        case .worktree(_, let path):
            TreeView(host: host, mac: mac, key: path)
        case .local(_, let id):
            TreeView(host: host, mac: mac, key: id)
        case .newSession(_, let project):
            NewSessionView(host: host, mac: mac, project: project, place: nil)
        case .automations:
            AutomationsView(host: host, mac: mac)
        default:
            HomeView(host: host, mac: mac)
        }
    }
}
