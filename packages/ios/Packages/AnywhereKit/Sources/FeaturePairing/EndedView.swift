import DesignSystem
import Stores
import SwiftUI

/// What a Mac whose link ended for good shows instead of its screens: removed from the Mac, or a side to update.
@MainActor
public struct EndedView: View {
    let app: AppModel
    let host: HostModel
    @State private var pairing = false
    @State private var forgetting = false

    public init(app: AppModel, host: HostModel) {
        self.app = app
        self.host = host
    }

    public var body: some View {
        let copy = Self.copy(host.reachability(at: .now), mac: host.name)
        VStack(spacing: Space.l) {
            Image(systemName: copy.systemImage)
                .font(.system(size: 44, weight: .light))
                .foregroundStyle(Palette.text3.color)
            VStack(spacing: Space.s) {
                Text(copy.title).font(Typography.title).foregroundStyle(Palette.text.color)
                Text(copy.message).font(Typography.subheadline).foregroundStyle(Palette.text3.color)
            }
            .multilineTextAlignment(.center)
            VStack(spacing: Space.s) {
                switch host.reachability(at: .now) {
                case .revoked:
                    Button("Pair again") { pairing = true }.buttonStyle(.borderedProminent)
                case .updatePhone:
                    Link("Open the App Store", destination: Self.appStore).buttonStyle(.borderedProminent)
                    Button("Try again") { app.restart(host.id) }.buttonStyle(.borderless)
                default:
                    Button("Try again") {
                        Haptics.tap()
                        app.restart(host.id)
                    }
                    .buttonStyle(.borderedProminent)
                }
                Button("Forget this Mac", role: .destructive) { forgetting = true }
                    .buttonStyle(.borderless)
                    .foregroundStyle(Palette.failedText.color)
                    .padding(.top, Space.xs)
            }
            .controlSize(.large)
        }
        .padding(Space.xxl)
        .frame(maxWidth: 480)
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .screenBackground()
        .sheet(isPresented: $pairing) {
            PairingView(app: app, knownHost: host.credentials.host) { pairing = false }
        }
        .confirmationDialog("Forget \(host.name)?", isPresented: $forgetting, titleVisibility: .visible) {
            Button("Forget", role: .destructive) { app.forget(host.id) }
        } message: {
            Text("This iPhone stops connecting to it. Pair again to bring it back.")
        }
    }

    struct Copy: Hashable {
        var systemImage: String
        var title: String
        var message: String
    }

    nonisolated static func copy(_ reachability: Reachability, mac: String) -> Copy {
        switch reachability {
        case .revoked:
            Copy(
                systemImage: "iphone.slash", title: "Removed from \(mac)",
                message: "This iPhone was removed in Anywhere on the Mac. Pair again with a new code to come back.")
        case .updateMac:
            Copy(
                systemImage: "arrow.down.circle", title: "Update Anywhere on \(mac)",
                message: "The Mac's Anywhere is older than this app. Update it on the Mac, then try again.")
        case .updatePhone:
            Copy(
                systemImage: "arrow.down.app", title: "Update this app",
                message: "\(mac) runs a newer Anywhere than this app speaks. Update it from the App Store.")
        default:
            Copy(systemImage: "bolt.horizontal.circle", title: "Disconnected from \(mac)", message: "Try connecting again.")
        }
    }

    /// The App Store app; its Updates list has Anywhere once an update is out.
    static let appStore = URL(string: "itms-apps://")!
}
