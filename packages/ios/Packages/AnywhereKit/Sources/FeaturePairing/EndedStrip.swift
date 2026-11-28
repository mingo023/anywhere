import DesignSystem
import Stores
import SwiftUI

/// Across a screen pushed for a Mac whose link ended for good: why, and the way back. What the screen shows stays
/// readable.
@MainActor
public struct EndedStrip: View {
    let app: AppModel
    let host: HostModel
    @State private var pairing = false
    @Environment(\.openURL) private var openURL

    public init(app: AppModel, host: HostModel) {
        self.app = app
        self.host = host
    }

    public var body: some View {
        let reachability = host.reachability(at: .now)
        if reachability.isFinal {
            NetStrip(.failed, EndedView.copy(reachability, mac: host.name).title, action: action(reachability))
                .padding(.horizontal, Inset.margin)
                .padding(.vertical, Space.xs)
                .sheet(isPresented: $pairing) {
                    PairingView(app: app, knownHost: host.credentials.host) { pairing = false }
                }
        }
    }

    private func action(_ reachability: Reachability) -> (title: String, run: () -> Void) {
        switch reachability {
        case .revoked: ("Pair again", { pairing = true })
        case .updatePhone: ("App Store", { openURL(EndedView.appStore) })
        default: ("Try again", {
            Haptics.tap()
            app.restart(host.id)
        })
        }
    }
}
