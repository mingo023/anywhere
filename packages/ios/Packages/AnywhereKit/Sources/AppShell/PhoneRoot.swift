import DesignSystem
import Domain
import FeatureGitHub
import FeaturePairing
import Stores
import SwiftUI

/// iPhone: Inbox, Sessions, PRs and Machines, each across every paired Mac with its own stack, under a floating glass
/// tab bar. Screens with their own pinned action or composer (a chat, a pull request, changes, files) cover the bar.
@MainActor
struct PhoneRoot: View {
    let app: AppModel
    @Bindable var navigator: Navigator

    var body: some View {
        #if os(iOS)
            TabView(selection: $navigator.tab) {
                ForEach(Tab.allCases, id: \.self) { tab in
                    section(tab).toolbar(.hidden, for: .tabBar).tag(tab)
                }
            }
        #else
            section(navigator.tab)
        #endif
    }

    private func section(_ tab: Tab) -> some View {
        NavigationStack(path: Binding(get: { navigator.path(tab) }, set: { navigator.setPath($0, for: tab) })) {
            Destination.root(tab, app: app, pair: pair, open: navigator.push, leave: navigator.leave)
                .routeDestinations(app: app, navigator: navigator, banner: true, pair: pair)
        }
        .safeAreaInset(edge: .top, spacing: 0) {
            if tab == navigator.tab, let text = HeldLinkRow.text(navigator.held, app: app) {
                HeldLinkRow(text: text, dismiss: navigator.dropHeld)
            }
        }
        .safeAreaInset(edge: .bottom, spacing: 0) {
            if Tab.showsBar(over: navigator.path(tab)) {
                GlassTabBar(TabItems.items(app.hosts), selection: $navigator.tab)
            }
        }
    }

    private func pair() { navigator.pairing = PairingRequest() }
}

/// The tabs' names, icons and marks: the Inbox counts what needs you on every Mac, PRs what needs action, and Machines
/// is dotted while a Mac can't be reached.
enum TabItems {
    @MainActor
    static func items(_ hosts: [HostModel]) -> [GlassTabBar<Tab>.Item] {
        let now = Date.now
        return [
            .init(.inbox, "Inbox", icon: .inbox, badge: hosts.reduce(0) { $0 + $1.sessions.needsYouCount }),
            .init(.sessions, "Sessions", icon: .sessions),
            .init(.prs, "PRs", icon: .pullRequest, badge: PullRequestsHome.needingAction(hosts)),
            .init(.machines, "Machines", icon: .laptop, dot: machinesDot(hosts.map { $0.reachability(at: now) })),
        ]
    }

    /// A Mac that is neither connected nor still dialling for the first time wants a look on Machines.
    nonisolated static func machinesDot(_ macs: [Reachability]) -> Bool {
        macs.contains { $0 != .online && $0 != .connecting }
    }
}
