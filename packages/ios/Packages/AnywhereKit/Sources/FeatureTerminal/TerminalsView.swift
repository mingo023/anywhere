import DesignSystem
import Domain
import Stores
import SwiftUI

/// A Mac's terminals, and one terminal attached.
@MainActor
public struct TerminalsView: View {
    let host: HostModel
    let route: Route

    public init(host: HostModel, route: Route) {
        self.host = host
        self.route = route
    }

    public var body: some View {
        switch route {
        case .terminal(let mac, let terminalId):
            TerminalScreenView(store: host.terminals, terminalId: terminalId, title: host.terminals.terminal(terminalId)?.title ?? "Shell", mac: mac)
        default:
            TerminalListView(host: host)
        }
    }
}
