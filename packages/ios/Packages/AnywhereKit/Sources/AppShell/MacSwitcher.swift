import DesignSystem
import Stores
import SwiftUI

extension View {
    /// The title's menu: the paired Macs to switch between, and pairing another.
    @ViewBuilder
    func macSwitcher(app: AppModel, navigator: Navigator, shown: Bool) -> some View {
        if shown {
            toolbarTitleMenu { MacMenu(app: app, navigator: navigator) }
        } else {
            self
        }
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
                Label(host.name, systemImage: host.id == navigator.macId ? "checkmark" : "laptopcomputer")
            }
        }
        Divider()
        Button("Pair another Mac", systemImage: "plus") { navigator.pairing = PairingRequest() }
    }
}
