import DesignSystem
import Domain
import Stores
import SwiftUI

/// Every paired Mac with its link and what its sessions want; each opens its home.
@MainActor
public struct MacsView: View {
    let app: AppModel

    public init(app: AppModel) {
        self.app = app
    }

    public var body: some View {
        List(app.hosts) { host in
            NavigationLink(value: Route.sessions(mac: host.hello?.serverId ?? host.id)) { MacRow(host: host) }
        }
        .navigationTitle("Macs")
        .toolbar {
            ToolbarItem(placement: .primaryAction) {
                NavigationLink(value: Route.pairing) { Label("Pair a Mac", systemImage: "plus") }
            }
        }
    }
}
