import DesignSystem
import Stores
import SwiftUI

/// What the chat's network strip says. Reconnecting and failed also dim the screen and turn off answering and sending;
/// "Back online" shows for two seconds and changes nothing else.
enum ChatNet: Hashable {
    case reconnecting
    case back
    case failed(asOf: Int?)

    nonisolated var frozen: Bool {
        if case .back = self { false } else { true }
    }

    /// The strip for a Mac's `reachability`: reconnecting once a drop lasts 4 s, failed after 30 s, and `back` while the
    /// Mac has just come back. A Mac gone for good is the shell's to say.
    nonisolated static func read(_ reachability: Reachability, back: Bool, asOf: Int?) -> ChatNet? {
        switch reachability {
        case .reconnecting: .reconnecting
        case .unreachable: .failed(asOf: asOf)
        case .online: back ? .back : nil
        case .connecting, .revoked, .updateMac, .updatePhone, .stopped: nil
        }
    }
}

/// The chat's one network strip.
@MainActor
struct ChatStrip: View {
    let net: ChatNet
    let mac: String
    let retry: () -> Void

    var body: some View {
        switch net {
        case .reconnecting: NetStrip(.reconnecting, NetStrip.reconnecting(mac: mac), glass: true)
        case .back: NetStrip(.back, "Back online", glass: true)
        case .failed(let asOf): NetStrip(.failed, NetStrip.failed(mac: mac, asOf: asOf), glass: true, action: ("Retry", retry))
        }
    }
}

/// Reads the Mac's link into the strip the screen shows, and redraws only when that reading can change. `forced` stands
/// in for a Mac in gallery scenes.
@MainActor
struct ChatNetwork<Content: View>: View {
    let host: HostModel?
    var forced: ChatNet?
    @ViewBuilder let content: (ChatNet?) -> Content
    @State private var back = false

    var body: some View {
        Group {
            if let host {
                let online = host.link == .online
                // The schedule only wakes the view at the marks; the clock is read fresh, as an entry's date may be an older mark.
                TimelineView(.explicit(host.reachabilityMarks)) { _ in
                    content(forced ?? ChatNet.read(host.reachability(at: .now), back: back && online, asOf: host.staleAsOf(at: .now)))
                }
            } else {
                content(forced)
            }
        }
        .onChange(of: host?.link == .online) { was, now in
            guard now, !was, host?.hello != nil else { return }
            back = true
        }
        .task(id: back) {
            guard back else { return }
            try? await Task.sleep(for: .seconds(2))
            back = false
        }
    }
}
