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

/// The app's root: pairing until a Mac is paired, then a stack on iPhone and a split view on iPad.
@MainActor
public struct RootView: View {
    let app: AppModel
    @State private var navigator = Navigator()
    @Environment(\.scenePhase) private var scenePhase
    #if os(iOS)
        @Environment(\.horizontalSizeClass) private var sizeClass
    #endif

    public init(app: AppModel) {
        self.app = app
    }

    public var body: some View {
        Group {
            if app.hosts.isEmpty {
                NavigationStack { PairingView(app: app) }
            } else if isWide {
                split
            } else {
                stack
            }
        }
        .tint(Palette.accent.color)
        .onOpenURL { url in
            guard let link = DeepLink.parse(url) else { return }
            navigator.open(link, macKnown: link.mac.map { app.host(mac: $0) != nil } ?? true)
        }
        .onChange(of: app.hosts.compactMap(\.hello?.serverId)) { _, macs in
            navigator.release(knownMacs: Set(macs))
        }
        .onChange(of: scenePhase) { _, phase in
            for host in app.hosts { host.scene(active: phase == .active) }
        }
    }

    private var isWide: Bool {
        #if os(iOS)
            sizeClass == .regular
        #else
            true
        #endif
    }

    private var stack: some View {
        NavigationStack(path: $navigator.path) {
            home.navigationDestination(for: Route.self, destination: destination)
        }
    }

    private var split: some View {
        NavigationSplitView {
            List(app.hosts) { host in
                NavigationLink(value: Route.sessions(mac: host.hello?.serverId ?? host.id)) {
                    Label(host.name, systemImage: "laptopcomputer")
                }
            }
            .navigationTitle("Macs")
        } detail: {
            NavigationStack(path: $navigator.path) {
                home.navigationDestination(for: Route.self, destination: destination)
            }
        }
    }

    @ViewBuilder
    private var home: some View {
        if app.hosts.count > 1 && !isWide {
            MacsView(app: app)
        } else if let host = app.hosts.first {
            SessionsView(host: host, route: .sessions(mac: host.hello?.serverId ?? host.id))
        }
    }

    @ViewBuilder
    private func destination(_ route: Route) -> some View {
        switch route {
        case .pairing:
            PairingView(app: app)
        case .settings:
            SettingsView(app: app)
        default:
            if let mac = route.mac, let host = app.host(mac: mac) ?? app.hosts.first(where: { $0.id == mac }) {
                Self.screen(route, host) { navigator.path.append($0) }
            } else {
                Placeholder("This Mac isn't connected", systemImage: "laptopcomputer.slash")
            }
        }
    }

    @ViewBuilder
    static func screen(_ route: Route, _ host: HostModel, open: @escaping (Route) -> Void = { _ in }) -> some View {
        switch route {
        case .sessions, .project, .worktree, .local, .newSession, .automations:
            SessionsView(host: host, route: route)
        case .chat(_, let agentId):
            ChatView(host: host, agentId: agentId, open: open)
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
}

extension DeepLink {
    var mac: String? {
        switch self {
        case .pair: nil
        case .mac(let mac), .agent(let mac, _): mac
        }
    }
}
