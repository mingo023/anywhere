import DesignSystem
import Stores
import SwiftUI

/// The screen's network strip for one Mac: "Reconnecting to <Mac>…" once a drop lasts 4 s, "Couldn't reconnect" with
/// Retry after 30 s, and "Back online" for 2 s once it's back. It redraws only when its reading can change.
@MainActor
public struct ConnectionBanner: View {
    let host: HostModel
    @State private var back = false

    public init(host: HostModel) {
        self.host = host
    }

    public var body: some View {
        Group {
            if let since = host.offlineSince, !host.reachability(at: since).isFinal {
                // The schedule only wakes the view at the marks; the clock is read fresh, as an entry's date may be an older mark.
                TimelineView(.explicit(host.reachabilityMarks)) { _ in
                    ConnectionBannerRow(name: host.name, asOf: host.staleAsOf(at: .now), reachability: host.reachability(at: .now)) {
                        Haptics.tap()
                        host.scene(active: true)
                    }
                }
            } else if back {
                NetStrip(.back, "Back online").stripPlacement()
            }
        }
        .onChange(of: host.offlineSince) { was, now in
            if now == nil, Reachability.backOnline(offlineSince: was, at: .now) { back = true }
        }
        .task(id: back) {
            guard back else { return }
            try? await Task.sleep(for: .seconds(2))
            back = false
        }
    }
}

@MainActor
struct ConnectionBannerRow: View {
    let name: String
    let asOf: Int?
    let reachability: Reachability
    let retry: () -> Void

    var body: some View {
        switch reachability {
        case .reconnecting:
            NetStrip(.reconnecting, NetStrip.reconnecting(mac: name)).stripPlacement()
        case .unreachable:
            NetStrip(.failed, NetStrip.failed(mac: name, asOf: asOf), action: ("Retry", retry)).stripPlacement()
        default:
            EmptyView()
        }
    }
}

extension View {
    fileprivate func stripPlacement() -> some View {
        padding(.horizontal, Inset.margin)
            .padding(.vertical, Space.xs)
            .transition(.move(edge: .top).combined(with: .opacity))
    }
}
