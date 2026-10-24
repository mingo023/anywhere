import DesignSystem
import Domain
import FeaturePairing
import Stores
import SwiftUI

/// iPhone: a tab per section, each with its own stack, scoped to the selected Mac. Git, GitHub and files live inside
/// a worktree's screens, since each needs a tree; a tab for them would repeat the sessions tree as a picker.
@MainActor
struct PhoneRoot: View {
    let app: AppModel
    @Bindable var navigator: Navigator

    var body: some View {
        TabView(selection: $navigator.tab) {
            section(.sessions)
                .tabItem { Label("Sessions", systemImage: "rectangle.stack") }
                .badge(host?.sessions.upNext.count ?? 0)
                .tag(Tab.sessions)
            section(.terminals)
                .tabItem { Label("Terminals", systemImage: "terminal") }
                .tag(Tab.terminals)
            section(.settings)
                .tabItem { Label("Settings", systemImage: "gearshape") }
                .tag(Tab.settings)
        }
    }

    private var host: HostModel? {
        navigator.macId.flatMap { id in app.hosts.first { $0.id == id } }
    }

    private func section(_ tab: Tab) -> some View {
        NavigationStack(path: Binding(get: { navigator.path(tab) }, set: { navigator.setPath($0, for: tab) })) {
            Destination.root(tab, host: host, app: app, pair: pair)
                .macSwitcher(app: app, navigator: navigator, shown: tab != .settings)
                .routeDestinations(app: app, pair: pair)
        }
        .safeAreaInset(edge: .top, spacing: 0) {
            if tab != .settings, let host { ConnectionBanner(host: host) }
        }
    }

    private func pair() { navigator.pairing = PairingRequest() }
}
