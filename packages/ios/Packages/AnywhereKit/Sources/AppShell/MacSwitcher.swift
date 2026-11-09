import DesignSystem
import Stores
import SwiftUI

extension View {
    /// The Mac in the bar's leading corner, a menu of the paired Macs to switch between and pairing another.
    @ViewBuilder
    func macSwitcher(app: AppModel, navigator: Navigator, shown: Bool) -> some View {
        if shown {
            environment(
                \.macSwitch,
                MacSwitch(
                    othersWaiting: OtherMacs.waiting(
                        Dictionary(app.hosts.map { ($0.id, $0.sessions.upNext.count) }, uniquingKeysWith: { $1 }), selected: navigator.macId)
                ) { AnyView(MacMenu(app: app, navigator: navigator)) })
        } else {
            self
        }
    }
}

enum OtherMacs {
    /// `upNext` is each Mac's Up next count by device id.
    nonisolated static func waiting(_ upNext: [String: Int], selected: String?) -> Bool {
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
