import Domain
import Stores
import SwiftUI

/// A Mac's sessions for the column beside the project tree (the iPad's content column): Up next, Working and other
/// folders, with search, New Session and Automations, but not the projects the sidebar holds.
@MainActor
public struct UpNextList: View {
    let host: HostModel
    let open: ((Route) -> Void)?

    public init(host: HostModel, open: ((Route) -> Void)? = nil) {
        self.host = host
        self.open = open
    }

    public var body: some View {
        HomeView(host: host, mac: host.mac, open: open, projects: false)
    }
}
