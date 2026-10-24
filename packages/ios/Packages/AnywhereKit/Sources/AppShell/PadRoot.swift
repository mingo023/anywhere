import DesignSystem
import Domain
import FeaturePairing
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
    }

    var body: some View {
        NavigationSplitView(columnVisibility: $columns) {
            List(selection: selection) {
                ForEach(app.hosts) { host in
                    Section {
                        NavigationLink(value: Item(tab: .sessions, mac: host.id)) {
                            Label("Sessions", systemImage: "rectangle.stack")
                        }
                        .badge(host.sessions.upNext.count)
                        NavigationLink(value: Item(tab: .terminals, mac: host.id)) {
                            Label("Terminals", systemImage: "terminal")
                        }
                        // INTEGRATION (workspace slice): the Mac's projects, worktrees and Locals go here.
                    } header: {
                        Text(host.name)
                    }
                }
                Section {
                    NavigationLink(value: Item(tab: .settings, mac: nil)) {
                        Label("Settings", systemImage: "gearshape")
                    }
                }
            }
            .navigationTitle("Anywhere")
            .navigationSplitViewColumnWidth(min: 220, ideal: 260)
        } content: {
            Destination.root(navigator.tab, host: host, app: app, pair: pair)
                .safeAreaInset(edge: .top, spacing: 0) {
                    if navigator.tab != .settings, let host { ConnectionBanner(host: host) }
                }
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
            get: { Item(tab: navigator.tab, mac: navigator.tab == .settings ? nil : navigator.macId) },
            set: { item in
                guard let item else { return }
                if let mac = item.mac { navigator.select(mac: mac) }
                navigator.tab = item.tab
            })
    }

    private func pair() { navigator.pairing = PairingRequest() }
}
