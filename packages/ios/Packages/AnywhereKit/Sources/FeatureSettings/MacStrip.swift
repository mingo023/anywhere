import DesignSystem
import Stores
import SwiftUI

/// The Macs list's network strip for one Mac: "Reconnecting to <Mac>…" once a drop lasts 4 s, "Couldn't reconnect" with
/// Retry after 30 s, and "Back online" for 2 s once it's back. A Mac whose link ended for good shows its own row's state.
@MainActor
struct MacStrip: View {
    let host: HostModel
    @State private var back: Bool

    init(host: HostModel, back: Bool = false) {
        self.host = host
        _back = State(initialValue: back)
    }

    var body: some View {
        Group {
            if let since = host.offlineSince, !host.reachability(at: since).isFinal {
                // The schedule only wakes the view at the marks; the clock is read fresh, as an entry's date may be an older mark.
                TimelineView(.explicit(host.reachabilityMarks)) { _ in strip(host.reachability(at: .now)) }
            } else if back {
                NetStrip(.back, "Back online").placed()
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

    @ViewBuilder
    private func strip(_ reachability: Reachability) -> some View {
        switch reachability {
        case .reconnecting:
            NetStrip(.reconnecting, NetStrip.reconnecting(mac: host.name)).placed()
        case .unreachable:
            NetStrip(.failed, NetStrip.failed(mac: host.name, asOf: host.staleAsOf(at: .now)), action: ("Retry", retry)).placed()
        default:
            EmptyView()
        }
    }

    private func retry() {
        Haptics.tap()
        host.scene(active: true)
    }
}

extension NetStrip {
    fileprivate func placed() -> some View {
        padding(.horizontal, Inset.margin)
            .padding(.top, Space.s)
            .transition(.move(edge: .top).combined(with: .opacity))
    }
}
