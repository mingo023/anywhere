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

    var body: some View {
        switch route {
        case .pairing:
            PairingView(app: app)
        case .settings:
            SettingsView(app: app, pair: pair)
        default:
            if let mac = route.mac, let host = app.host(mac: mac) {
                Self.screen(route, host)
            } else {
                Placeholder("This Mac isn't paired", systemImage: "laptopcomputer.slash", detail: "Pair it again from Settings.")
            }
        }
    }

    @ViewBuilder
    static func screen(_ route: Route, _ host: HostModel) -> some View {
        switch route {
        case .sessions, .project, .worktree, .local, .newSession, .automations:
            SessionsView(host: host, route: route)
        case .chat(_, let agentId):
            ChatView(host: host, agentId: agentId)
        case .terminals, .terminal:
            TerminalsView(host: host, route: route)
        case .changes, .diff, .history, .commit, .branches:
            GitView(host: host, route: route)
        case .pullRequest, .pullRequests:
            GitHubView(host: host, route: route)
        case .files, .file:
            FilesView(host: host, route: route)
        case .pairing, .settings:
            EmptyView()
        }
    }

    /// The root of a section for a Mac; a Mac whose link ended shows why instead.
    @ViewBuilder
    static func root(_ tab: Tab, host: HostModel?, app: AppModel, pair: @escaping @MainActor () -> Void) -> some View {
        switch tab {
        case .settings:
            SettingsView(app: app, pair: pair)
        case .sessions, .terminals:
            if let host {
                if host.reachability(at: .now).isFinal {
                    EndedView(app: app, host: host)
                } else if tab == .sessions {
                    SessionsView(host: host, route: .sessions(mac: host.mac))
                } else {
                    TerminalsView(host: host, route: .terminals(mac: host.mac))
                }
            } else {
                Placeholder("No Mac selected", systemImage: "laptopcomputer")
            }
        }
    }
}

extension View {
    /// Every `Route` a stack can push.
    func routeDestinations(app: AppModel, pair: @escaping @MainActor () -> Void) -> some View {
        navigationDestination(for: Route.self) { Destination(route: $0, app: app, pair: pair) }
    }
}
