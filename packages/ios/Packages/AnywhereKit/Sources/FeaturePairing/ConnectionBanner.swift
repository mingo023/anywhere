import DesignSystem
import Stores
import SwiftUI

/// "Reconnecting to <Mac>…" once a drop lasts 4 s, and "Can't reach <Mac>" with a Tailscale hint after 30 s.
/// It redraws only when its reading can change: on a link change and at those two marks.
@MainActor
public struct ConnectionBanner: View {
    let host: HostModel

    public init(host: HostModel) {
        self.host = host
    }

    public var body: some View {
        if let since = host.offlineSince, !host.reachability(at: since).isFinal {
            // The schedule only wakes the view at the marks; the clock is read fresh, as an entry's date may be an older mark.
            TimelineView(.explicit([since, since + Reachability.reconnectingAfter, since + Reachability.unreachableAfter])) { _ in
                ConnectionBannerRow(name: host.name, reachability: host.reachability(at: .now)) {
                    Haptics.tap()
                    host.scene(active: true)
                }
            }
        }
    }
}

@MainActor
struct ConnectionBannerRow: View {
    let name: String
    let reachability: Reachability
    let retry: () -> Void

    var body: some View {
        switch reachability {
        case .reconnecting:
            HStack(spacing: Space.s) {
                ProgressView().controlSize(.small)
                Text("Reconnecting to \(name)…").font(Typography.subheadline).foregroundStyle(Palette.text.color).lineLimit(1)
            }
            .bannerChrome()
            .transition(.move(edge: .top).combined(with: .opacity))
        case .unreachable:
            HStack(spacing: Space.m) {
                Image(systemName: "wifi.exclamationmark").foregroundStyle(Palette.waitingText.color)
                VStack(alignment: .leading, spacing: 0) {
                    Text("Can't reach \(name)").font(Typography.subheadline.weight(.semibold)).foregroundStyle(Palette.text.color)
                    Text("Check Tailscale is on here and on the Mac.").font(Typography.caption).foregroundStyle(Palette.text3.color)
                }
                .lineLimit(1)
                Spacer(minLength: Space.s)
                Button("Retry", action: retry).font(Typography.subheadline.weight(.semibold))
            }
            .bannerChrome()
            .transition(.move(edge: .top).combined(with: .opacity))
            .accessibilityElement(children: .combine)
        default:
            EmptyView()
        }
    }
}

extension View {
    fileprivate func bannerChrome() -> some View {
        padding(.horizontal, Space.l)
            .padding(.vertical, Space.s + 2)
            .glass(in: Capsule())
            .shadow(color: .black.opacity(0.08), radius: 12, y: 4)
            .padding(.horizontal, Space.l)
            .padding(.top, Space.xs)
    }
}
