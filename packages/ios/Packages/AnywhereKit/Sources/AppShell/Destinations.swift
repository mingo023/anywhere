import DesignSystem
import Domain
import FeatureChat
import FeatureFiles
import FeatureGit
import FeatureGitHub
import FeaturePairing
import FeatureSessions
import FeatureSettings
import FeatureTerminal
import Stores
import SwiftUI

/// The one place a `Route` meets the feature view that shows it. Features navigate with `NavigationLink(value: Route…)`
/// and never import each other; when a feature's entry view changes, only this file follows.
@MainActor
struct Destination: View {
    let route: Route
    let app: AppModel
    let pair: @MainActor () -> Void
    let open: @MainActor (Route) -> Void
    let leave: @MainActor (String) -> Void
    /// Shows the reconnecting banner under the bar; the iPad's column shows its own.
    var banner = false

    var body: some View {
        switch route {
        case .pairing:
            PairingView(app: app)
        case .settings:
            SettingsView(app: app, pair: pair)
        default:
            if let mac = route.mac, let host = app.host(mac: mac) {
                // A Mac dialed again is a new host, so the screen's stores bind to it afresh.
                Self.screen(route, host, open: open, leave: leave)
                    .id(ObjectIdentifier(host))
                    .safeAreaInset(edge: .top, spacing: 0) {
                        VStack(spacing: 0) {
                            if banner { ConnectionBanner(host: host) }
                            EndedStrip(app: app, host: host)
                        }
                    }
            } else {
                Placeholder("This Mac isn't paired", icon: .laptop, detail: "Pair it again from Machines.")
            }
        }
    }

    @ViewBuilder
    static func screen(
        _ route: Route, _ host: HostModel, open: @escaping @MainActor (Route) -> Void, leave: @escaping @MainActor (String) -> Void
    ) -> some View {
        switch route {
        case .sessions, .project, .worktree, .local, .newSession, .automations:
            SessionsView(host: host, route: route, open: open, leave: leave)
        case .chat(_, let agentId):
            ChatView(host: host, agentId: agentId, open: open)
        case .subAgent(_, let agentId, let callId):
            SubAgentView(host: host, agentId: agentId, callId: callId, open: open)
        case .terminals, .terminal:
            TerminalsView(host: host, route: route)
        case .changes, .diff, .history, .commit, .branches:
            GitView(host: host, route: route, open: open)
        case .pullRequest, .pullRequests, .reviewThreads:
            GitHubView(host: host, route: route, open: open, leave: leave)
        case .files, .file:
            FilesView(host: host, route: route, open: open)
        case .pairing, .settings:
            EmptyView()
        }
    }

    /// The root of a section: Inbox, Sessions and PRs across every paired Mac, and Machines.
    @ViewBuilder
    static func root(
        _ tab: Tab, app: AppModel, pair: @escaping @MainActor () -> Void, open: @escaping @MainActor (Route) -> Void,
        leave: @escaping @MainActor (String) -> Void
    ) -> some View {
        switch tab {
        case .inbox: InboxView(hosts: app.hosts, open: open)
        case .sessions: SessionsHome(hosts: app.hosts, open: open)
        case .prs: PullRequestsHome(hosts: app.hosts, open: open)
        case .machines: SettingsView(app: app, pair: pair)
        }
    }
}

extension View {
    /// Every `Route` a stack can push.
    func routeDestinations(app: AppModel, navigator: Navigator, banner: Bool = false, pair: @escaping @MainActor () -> Void) -> some View {
        navigationDestination(for: Route.self) {
            // The iPad swaps the route in place: without the id, the next chat keeps the last one's store.
            Destination(route: $0, app: app, pair: pair, open: navigator.push, leave: navigator.leave, banner: banner).id($0)
        }
    }
}
