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
                        MacPill(host: navigator.macId.flatMap { id in app.hosts.first { $0.id == id } })
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

/// The selected Mac: a laptop dotted with its link, and its name.
@MainActor
struct MacPill: View {
    let host: HostModel?

    var body: some View {
        HStack(spacing: Space.s) {
            Image(systemName: "laptopcomputer")
                .overlay(alignment: .bottomTrailing) {
                    Circle().fill((host?.link == .online ? Palette.success : Palette.text4).color).frame(width: 7, height: 7)
                        .offset(x: 2, y: 2)
                }
            Text(host?.name ?? "No Mac").lineLimit(1)
            Image(systemName: "chevron.down").font(Typography.caption.weight(.semibold)).foregroundStyle(Palette.text3.color)
        }
        .font(Typography.subheadline.weight(.medium))
        .padding(.horizontal, Space.xs)
        .accessibilityLabel("Mac: \(host?.name ?? "none")")
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
