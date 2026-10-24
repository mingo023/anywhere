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
        Placeholder("Terminals", systemImage: "terminal")
    }
}
