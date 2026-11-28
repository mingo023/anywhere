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
        let reachability = host.reachability(at: .now)
        let copy = Self.copy(reachability, mac: host.name)
        VStack(spacing: Space.l) {
            LucideIcon(copy.icon, size: 40, weight: 1.75).foregroundStyle(Palette.text3.color)
            VStack(spacing: Space.s) {
                Text(copy.title).font(.system(size: 22, weight: .bold)).foregroundStyle(Palette.text.color)
                Text(copy.message).font(.system(size: 15)).foregroundStyle(Palette.text2.color)
            }
            .multilineTextAlignment(.center)
            VStack(spacing: Space.xs) {
                switch reachability {
                case .revoked:
                    Button("Pair again") { pairing = true }.buttonStyle(LargeButtonStyle())
                case .updatePhone:
                    Link("Open the App Store", destination: Self.appStore).buttonStyle(LargeButtonStyle())
                    Button("Try again") { app.restart(host.id) }.buttonStyle(QuietButtonStyle())
                default:
                    Button("Try again") {
                        Haptics.tap()
                        app.restart(host.id)
                    }
                    .buttonStyle(LargeButtonStyle())
                }
                Button("Forget this machine", role: .destructive) { forgetting = true }
                    .buttonStyle(QuietButtonStyle(destructive: true))
            }
        }
        .padding(.horizontal, Space.xl)
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
        var icon: Lucide
        var title: String
        var message: String
    }

    nonisolated static func copy(_ reachability: Reachability, mac: String) -> Copy {
        switch reachability {
        case .revoked:
            Copy(
                icon: .unlink, title: "Removed from \(mac)",
                message: "This iPhone was removed in Zoes on the Mac. Pair again with a new code to come back.")
        case .updateMac:
            Copy(
                icon: .arrowDown, title: "Update Zoes on \(mac)",
                message: "The Mac's Zoes is older than this app. Update it on the Mac, then try again.")
        case .updatePhone:
            Copy(
                icon: .arrowDown, title: "Update this app",
                message: "\(mac) runs a newer Zoes than this app speaks. Update it from the App Store.")
        default:
            Copy(icon: .bolt, title: "Disconnected from \(mac)", message: "Try connecting again.")
        }
    }

    /// The App Store app; its Updates list has Zoes once an update is out.
    static let appStore = URL(string: "itms-apps://")!
}
