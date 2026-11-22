import DesignSystem
import Foundation
import Stores
import SwiftUI

/// The list before a Mac's first answer: a spinner while the banner above says it is reconnecting, then why and Try Again
/// once the banner says it can't reach the Mac. A link ended for good leaves it to the strip above.
@MainActor
struct FirstLoad: View {
    let host: HostModel

    var body: some View {
        TimelineView(.periodic(from: .now, by: 1)) { context in
            let reachability = host.reachability(at: context.date)
            if Self.waits(reachability) {
                ProgressView()
            } else if !reachability.isFinal {
                Placeholder("Can't reach \(host.name)", systemImage: "wifi.slash", detail: "Is your Mac awake and on Tailscale?") {
                    Task { await host.connection.reconnectNow() }
                }
            }
        }
    }

    nonisolated static func waits(_ reachability: Reachability) -> Bool {
        switch reachability {
        case .online, .connecting, .reconnecting: true
        default: false
        }
    }
}
