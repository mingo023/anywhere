import DesignSystem
import Domain
import FeaturePairing
import FeatureSessions
import Stores
import SwiftUI

/// iPad: a sidebar of Macs and their sections, the section's list in the middle, and the chat, diff or file it opens on
/// the right. It shares the iPhone's `Navigator`, so a size-class change keeps the place.
@MainActor
struct PadRoot: View {
    let app: AppModel
    @Bindable var navigator: Navigator
    @State private var columns = NavigationSplitViewVisibility.all

    struct Item: Hashable {
        var tab: Tab
        /// nil for Settings, which belongs to no Mac.
        var mac: String?
        /// The project, worktree or Local the sessions list is narrowed to.
        var tree: Route?
    }

    var body: some View {
        NavigationSplitView(columnVisibility: $columns) {
            PadSidebar(app: app, selection: selection, macId: navigator.macId)
        } content: {
            Destination.root(navigator.tab, host: host, app: app, tree: navigator.tree, upNext: true, pair: pair, open: navigator.push,
                             leave: navigator.leave)
                .safeAreaInset(edge: .top, spacing: 0) {
                    VStack(spacing: 0) {
                        if navigator.tab != .settings, let host { ConnectionBanner(host: host) }
                        if navigator.tab == .sessions, let text = HeldLinkRow.text(navigator.held, app: app) {
                            HeldLinkRow(text: text, dismiss: navigator.dropHeld)
                        }
                    }
                }
                .environment(\.showInDetail) { navigator.setPath([$0], for: navigator.tab) }
                .navigationSplitViewColumnWidth(min: 320, ideal: 380)
        } detail: {
            NavigationStack(path: Binding(get: { navigator.path(navigator.tab) }, set: { navigator.setPath($0, for: navigator.tab) })) {
                Placeholder(
                    navigator.tab == .settings ? "Pick a Mac" : "Pick a session", systemImage: "sidebar.squares.left",
                    detail: navigator.tab == .terminals ? "Or open a terminal." : nil
                )
                .routeDestinations(app: app, navigator: navigator, pair: pair)
            }
        }
    }

    private var host: HostModel? {
        navigator.macId.flatMap { id in app.hosts.first { $0.id == id } }
    }

    private var selection: Binding<Item?> {
        Binding(
            get: { Item(tab: navigator.tab, mac: navigator.tab == .settings ? nil : navigator.macId, tree: navigator.tab == .sessions ? navigator.tree : nil) },
            set: { item in
                guard let item else { return }
                if let mac = item.mac { navigator.select(mac: mac) }
                navigator.tab = item.tab
                navigator.tree = item.tree
            })
    }

    private func pair() { navigator.pairing = PairingRequest() }
}
