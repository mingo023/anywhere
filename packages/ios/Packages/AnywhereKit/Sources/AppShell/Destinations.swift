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

    var body: some View {
        switch route {
        case .pairing:
            PairingView(app: app)
        case .settings:
            SettingsView(app: app, pair: pair)
        default:
            if let mac = route.mac, let host = app.host(mac: mac) {
                // A Mac dialed again is a new host, so the screen's stores bind to it afresh.
                Self.screen(route, host, open: open)
                    .id(ObjectIdentifier(host))
                    .safeAreaInset(edge: .top, spacing: 0) { EndedStrip(app: app, host: host) }
            } else {
                Placeholder("This Mac isn't paired", systemImage: "laptopcomputer.slash", detail: "Pair it again from Settings.")
            }
        }
    }

    @ViewBuilder
    static func screen(_ route: Route, _ host: HostModel, open: @escaping @MainActor (Route) -> Void) -> some View {
        switch route {
        case .sessions, .project, .worktree, .local, .newSession, .automations:
            SessionsView(host: host, route: route, open: open)
        case .chat(_, let agentId):
            ChatView(host: host, agentId: agentId, open: open)
                #if os(iOS)
                    // A session takes the screen: the composer sits where the tab bar would.
                    .toolbar(.hidden, for: .tabBar)
                #endif
        case .subAgent(_, let agentId, let callId):
            SubAgentView(host: host, agentId: agentId, callId: callId, open: open)
                #if os(iOS)
                    .toolbar(.hidden, for: .tabBar)
                #endif
        case .terminals, .terminal:
            TerminalsView(host: host, route: route)
        case .changes, .diff, .history, .commit, .branches:
            GitView(host: host, route: route)
        case .pullRequest, .pullRequests, .reviewThreads:
            GitHubView(host: host, route: route)
        case .files, .file:
            FilesView(host: host, route: route, open: open)
        case .pairing, .settings:
            EmptyView()
        }
    }

    /// The root of a section for a Mac, its sessions narrowed to `tree` if given; a Mac whose link ended shows why instead.
    /// With `upNext`, a Mac's sessions are its Up next and Working only, as the iPad's sidebar holds its projects.
    @ViewBuilder
    static func root(
        _ tab: Tab, host: HostModel?, app: AppModel, tree: Route? = nil, upNext: Bool = false, pair: @escaping @MainActor () -> Void,
        open: @escaping @MainActor (Route) -> Void
    ) -> some View {
        switch tab {
        case .settings:
            SettingsView(app: app, pair: pair)
        case .sessions, .terminals:
            if let host {
                if host.reachability(at: .now).isFinal {
                    EndedView(app: app, host: host)
                } else if tab == .sessions {
                    if upNext && tree == nil {
                        UpNextList(host: host)
                    } else {
                        SessionsView(host: host, route: tree ?? .sessions(mac: host.mac), open: open)
                    }
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
    func routeDestinations(app: AppModel, navigator: Navigator, pair: @escaping @MainActor () -> Void) -> some View {
        navigationDestination(for: Route.self) { Destination(route: $0, app: app, pair: pair, open: navigator.push) }
    }
}
