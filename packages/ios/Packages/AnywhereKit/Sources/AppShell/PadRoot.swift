import DesignSystem
import Domain
import FeaturePairing
import FeatureSessions
import FeatureSettings
import Stores
import SwiftUI

/// iPad: a sidebar of the four sections and each Mac's projects, the section's list in the middle, and the chat, diff or
/// file it opens on the right; Machines takes both of those columns. It shares the iPhone's `Navigator`, so a size-class
/// change keeps the place.
@MainActor
struct PadRoot: View {
    let app: AppModel
    @Bindable var navigator: Navigator
    @State private var columns = NavigationSplitViewVisibility.all
    /// Here, as the sidebar is built anew between Machines' two columns and the sections' three.
    @State private var folds = PadSidebar.Folds()

    struct Item: Hashable {
        var tab: Tab
        /// The project, worktree or Local the sessions list is narrowed to.
        var tree: Route?
    }

    var body: some View {
        if navigator.tab == .machines { machines } else { sections }
    }

    private var sidebar: some View {
        PadSidebar(app: app, selection: selection, macId: navigator.macId, folds: $folds)
    }

    private var machines: some View {
        NavigationSplitView(columnVisibility: $columns) {
            sidebar
        } detail: {
            NavigationStack(path: path) {
                SettingsView(app: app, pair: pair).routeDestinations(app: app, navigator: navigator, pair: pair)
            }
        }
    }

    private var sections: some View {
        NavigationSplitView(columnVisibility: $columns) {
            sidebar
        } content: {
            content
                .safeAreaInset(edge: .top, spacing: 0) {
                    if let text = HeldLinkRow.text(navigator.held, app: app) {
                        HeldLinkRow(text: text, dismiss: navigator.dropHeld)
                    }
                }
                .environment(\.showInDetail, ShowInDetail(owner: navigator) { navigator.setPath([$0], for: navigator.tab) })
                .navigationSplitViewColumnWidth(min: 320, ideal: 380)
        } detail: {
            NavigationStack(path: path) {
                Placeholder("Pick a session", icon: .sidebar)
                    .routeDestinations(app: app, navigator: navigator, banner: true, pair: pair)
            }
        }
    }

    /// The section's list, or the sessions of the tree it's narrowed to.
    @ViewBuilder
    private var content: some View {
        if navigator.tab == .sessions, let tree = navigator.tree, let mac = tree.mac, let host = app.host(mac: mac) {
            SessionsView(host: host, route: tree, open: navigator.push, leave: navigator.leave)
                .safeAreaInset(edge: .top, spacing: 0) { ConnectionBanner(host: host) }
        } else {
            Destination.root(navigator.tab, app: app, pair: pair, open: navigator.push, leave: navigator.leave)
        }
    }

    private var path: Binding<[Route]> {
        Binding(get: { navigator.path(navigator.tab) }, set: { navigator.setPath($0, for: navigator.tab) })
    }

    private var selection: Binding<Item?> {
        Binding(
            get: { Item(tab: navigator.tab, tree: navigator.tab == .sessions ? navigator.tree : nil) },
            set: { item in
                guard let item else { return }
                navigator.tab = item.tab
                guard item.tab == .sessions else { return }
                if let mac = item.tree?.mac, let host = app.host(mac: mac) {
                    navigator.select(mac: host.id)
                    navigator.select(place: item.tree, chats: host.chats(in: item.tree!))
                } else {
                    navigator.select(place: nil, chats: [])
                }
            })
    }

    private func pair() { navigator.pairing = PairingRequest() }
}
