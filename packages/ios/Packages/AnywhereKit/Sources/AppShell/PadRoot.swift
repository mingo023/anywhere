import DesignSystem
import Domain
import FeaturePairing
import FeatureSettings
import FeatureSessions
import Stores
import SwiftUI

/// iPad: a sidebar of Macs and their sections, the section's list in the middle, and the chat, diff or file it opens on
/// the right; Settings takes both of those columns. It shares the iPhone's `Navigator`, so a size-class change keeps the place.
@MainActor
struct PadRoot: View {
    let app: AppModel
    @Bindable var navigator: Navigator
    @State private var columns = NavigationSplitViewVisibility.all
    /// Here, as the sidebar is built anew between Settings' two columns and the sections' three.
    @State private var folds = PadSidebar.Folds()

    struct Item: Hashable {
        var tab: Tab
        /// nil for Settings, which belongs to no Mac.
        var mac: String?
        /// The project, worktree or Local the sessions list is narrowed to.
        var tree: Route?
    }

    var body: some View {
        if navigator.tab == .settings { settings } else { sections }
    }

    private var sidebar: some View {
        PadSidebar(app: app, selection: selection, macId: navigator.macId, folds: $folds)
    }

    private var settings: some View {
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
            Destination.root(navigator.tab, host: host, app: app, tree: navigator.tree, upNext: true, pair: pair, open: navigator.push,
                             leave: navigator.leave)
                .safeAreaInset(edge: .top, spacing: 0) {
                    VStack(spacing: 0) {
                        if let host { ConnectionBanner(host: host) }
                        if navigator.tab == .sessions, let text = HeldLinkRow.text(navigator.held, app: app) {
                            HeldLinkRow(text: text, dismiss: navigator.dropHeld)
                        }
                    }
                }
                .environment(\.showInDetail) { navigator.setPath([$0], for: navigator.tab) }
                .navigationSplitViewColumnWidth(min: 320, ideal: 380)
        } detail: {
            NavigationStack(path: path) {
                Placeholder("Pick a session", systemImage: "sidebar.squares.left")
                .routeDestinations(app: app, navigator: navigator, pair: pair)
            }
        }
    }

    private var path: Binding<[Route]> {
        Binding(get: { navigator.path(navigator.tab) }, set: { navigator.setPath($0, for: navigator.tab) })
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
                let chats = item.tree.flatMap { place in host?.chats(in: place) } ?? []
                navigator.select(place: item.tree, chats: chats)
            })
    }

    private func pair() { navigator.pairing = PairingRequest() }
}
