import DesignSystem
import Domain
import Stores
import SwiftUI

/// Projects, worktrees and Locals with their sessions, Up next, new sessions and automations.
@MainActor
public struct SessionsView: View {
    let host: HostModel
    let route: Route
    let open: ((Route) -> Void)?
    /// Leaves the screens of a tree being deleted.
    let leave: ((String) -> Void)?

    public init(host: HostModel, route: Route, open: ((Route) -> Void)? = nil, leave: ((String) -> Void)? = nil) {
        self.host = host
        self.route = route
        self.open = open
        self.leave = leave
    }

    public var body: some View {
        let mac = route.mac ?? host.hello?.serverId ?? host.id
        switch route {
        case .project(_, let path):
            HomeView(host: host, mac: mac, only: path, open: open)
        case .worktree(_, let path):
            TreeView(host: host, mac: mac, key: path, open: open, leave: leave)
        case .local(_, let id):
            TreeView(host: host, mac: mac, key: id, open: open, leave: leave)
        case .newSession(_, let project, let place, let prompt):
            // A sheet's content, with its own stack: the shell presents this route over the screen that asked.
            MakingSheet(host: host, mac: mac, making: .session(project: project, place: place.map { Places.place(key: $0, locals: host.workspace.locals) },
                                                               prompt: prompt ?? ""))
        case .automations:
            AutomationsView(host: host, mac: mac)
        default:
            HomeView(host: host, mac: mac, open: open)
        }
    }
}
