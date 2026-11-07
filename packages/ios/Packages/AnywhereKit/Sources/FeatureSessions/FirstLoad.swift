import DesignSystem
import Foundation
import Stores
import SwiftUI

/// The list before a Mac's first answer: a spinner, then, once the Mac is past a blip away, why and Try Again.
@MainActor
struct FirstLoad: View {
    let host: HostModel

    var body: some View {
        TimelineView(.periodic(from: .now, by: 1)) { context in
            switch host.reachability(at: context.date) {
            case .online, .connecting:
                ProgressView()
            default:
                Placeholder("Can't reach \(host.name)", systemImage: "wifi.slash", detail: "Is your Mac awake and on Tailscale?") {
                    Task { await host.connection.reconnectNow() }
                }
            }
        }
    }
}
