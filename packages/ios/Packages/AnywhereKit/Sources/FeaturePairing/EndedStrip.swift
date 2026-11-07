import DesignSystem
import Stores
import SwiftUI

/// Across a screen pushed for a Mac whose link ended for good: why, and the way back, as `EndedView` says it. What the
/// screen shows stays readable.
@MainActor
public struct EndedStrip: View {
    let app: AppModel
    let host: HostModel
    @State private var pairing = false

    public init(app: AppModel, host: HostModel) {
        self.app = app
        self.host = host
    }

    public var body: some View {
        let reachability = host.reachability(at: .now)
        if reachability.isFinal {
            let copy = EndedView.copy(reachability, mac: host.name)
            HStack(spacing: Space.m) {
                Image(systemName: copy.systemImage).foregroundStyle(Palette.failedText.color)
                VStack(alignment: .leading, spacing: Space.xxs) {
                    Text(copy.title).font(Typography.footnote.weight(.semibold)).foregroundStyle(Palette.text.color)
                    Text(copy.message).font(Typography.caption).foregroundStyle(Palette.text3.color).lineLimit(2)
                }
                Spacer(minLength: Space.s)
                action(reachability)
                    .font(Typography.subheadline.weight(.semibold))
                    .buttonStyle(.borderless)
            }
            .padding(.horizontal, Space.l)
            .padding(.vertical, Space.s)
            .frame(maxWidth: .infinity, minHeight: HitTarget.minimum)
            .background(Palette.failedBackground.color)
            .background(Palette.surface.color)
            .sheet(isPresented: $pairing) {
                PairingView(app: app, knownHost: host.credentials.host) { pairing = false }
            }
        }
    }

    @ViewBuilder
    private func action(_ reachability: Reachability) -> some View {
        switch reachability {
        case .revoked:
            Button("Pair again") { pairing = true }
        case .updatePhone:
            Link("App Store", destination: EndedView.appStore)
        default:
            Button("Try again") {
                Haptics.tap()
                app.restart(host.id)
            }
        }
    }
}
