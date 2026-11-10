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

    init(app: AppModel, navigator: Navigator) {
        self.app = app
        self.navigator = navigator
        _ = Self.inkBadge
    }

    /// The tab's Up next count in ink: colour is for status, and SwiftUI has no way to set a badge's. Appearance is
    /// process-wide, so once.
    private static let inkBadge: Void = {
        #if os(iOS)
            let appearance = UITabBarAppearance()
            appearance.configureWithDefaultBackground()
            for layout in [appearance.stackedLayoutAppearance, appearance.inlineLayoutAppearance, appearance.compactInlineLayoutAppearance] {
                layout.normal.badgeBackgroundColor = UIColor(Palette.text.color)
                layout.normal.badgeTextAttributes = [.foregroundColor: UIColor(Palette.onText.color)]
            }
            UITabBar.appearance().standardAppearance = appearance
            UITabBar.appearance().scrollEdgeAppearance = appearance
        #endif
    }()

    var body: some View {
        TabView(selection: $navigator.tab) {
            section(.sessions)
                .tabItem { Label("Sessions", systemImage: "rectangle.stack") }
                .badge(host?.sessions.upNext.count ?? 0)
                .tag(Tab.sessions)
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
            Destination.root(tab, host: host, app: app, pair: pair, open: navigator.push, leave: navigator.leave)
                .macSwitcher(app: app, navigator: navigator, shown: tab != .settings)
                .routeDestinations(app: app, navigator: navigator, pair: pair)
        }
        .safeAreaInset(edge: .top, spacing: 0) {
            VStack(spacing: 0) {
                if tab != .settings, let host { ConnectionBanner(host: host) }
                if tab == .sessions, let text = HeldLinkRow.text(navigator.held, app: app) {
                    HeldLinkRow(text: text, dismiss: navigator.dropHeld)
                }
            }
        }
    }

    private func pair() { navigator.pairing = PairingRequest() }
}
