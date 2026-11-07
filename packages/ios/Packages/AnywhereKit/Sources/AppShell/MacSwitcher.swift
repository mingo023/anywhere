import DesignSystem
import Stores
import SwiftUI

extension View {
    /// The Mac in the bar's leading corner, a menu of the paired Macs to switch between and pairing another.
    @ViewBuilder
    func macSwitcher(app: AppModel, navigator: Navigator, shown: Bool) -> some View {
        if shown {
            toolbar {
                ToolbarItem(placement: Self.leading) {
                    Menu {
                        MacMenu(app: app, navigator: navigator)
                    } label: {
                        MacPill(
                            host: navigator.macId.flatMap { id in app.hosts.first { $0.id == id } },
                            othersWaiting: MacPill.othersWaiting(
                                Dictionary(app.hosts.map { ($0.id, $0.sessions.upNext.count) }, uniquingKeysWith: { $1 }), selected: navigator.macId))
                    }
                }
            }
        } else {
            self
        }
    }

    private static var leading: ToolbarItemPlacement {
        #if os(iOS)
            .topBarLeading
        #else
            .navigation
        #endif
    }
}

/// The selected Mac: a laptop dotted with its link, its name, and a dot when another Mac has sessions Up next.
@MainActor
struct MacPill: View {
    let host: HostModel?
    var othersWaiting = false

    var body: some View {
        HStack(spacing: Space.s) {
            Image(systemName: "laptopcomputer")
                .overlay(alignment: .bottomTrailing) {
                    Circle().fill((host?.link == .online ? Palette.success : Palette.text4).color).frame(width: 7, height: 7)
                        .offset(x: 2, y: 2)
                }
            Text(host?.name ?? "No Mac").lineLimit(1)
            if othersWaiting { Circle().fill(Palette.waitingDot.color).frame(width: 7, height: 7) }
            Image(systemName: "chevron.down").font(Typography.caption.weight(.semibold)).foregroundStyle(Palette.text3.color)
        }
        .font(Typography.subheadline.weight(.medium))
        .padding(.horizontal, Space.xs)
        .accessibilityLabel("Mac: \(host?.name ?? "none")")
        .accessibilityValue(othersWaiting ? "Another Mac has sessions up next" : "")
    }

    /// `upNext` is each Mac's Up next count by device id.
    nonisolated static func othersWaiting(_ upNext: [String: Int], selected: String?) -> Bool {
        upNext.contains { $0.key != selected && $0.value > 0 }
    }
}

@MainActor
struct MacMenu: View {
    let app: AppModel
    let navigator: Navigator

    var body: some View {
        ForEach(app.hosts) { host in
            Button {
                Haptics.tap()
                navigator.select(mac: host.id)
            } label: {
                Label {
                    Text(host.name)
                    Text(Self.detail(upNext: host.sessions.upNext.count, reachability: host.reachability(at: .now)))
                } icon: {
                    Image(systemName: host.id == navigator.macId ? "checkmark" : "laptopcomputer")
                }
            }
        }
        Divider()
        Button("Pair another Mac", systemImage: "plus") { navigator.pairing = PairingRequest() }
    }

    /// A Mac's line under its name: what waits on it and how its link reads.
    nonisolated static func detail(upNext: Int, reachability: Reachability) -> String {
        upNext > 0 ? "\(upNext) up next · \(reachability.label)" : reachability.label
    }
}
