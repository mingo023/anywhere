import Client
import DesignSystem
import Fixtures
import Foundation
import Stores
import SwiftUI

public enum PairingGallery {
    static let link = PairingLink(host: "100.77.122.82:4517", code: "q3xYq3xYq3xYq3xYq3xY_-", name: Sample.hello.hostname)

    @MainActor public static var scenes: [GalleryScene] {
        [false, true].flatMap { dark in
            let suffix = dark ? "-dark" : ""
            return [
                GalleryScene("pairing-welcome\(suffix)", dark: dark) { AnyView(PairingView(model: model())) },
                GalleryScene("pairing-enter\(suffix)", dark: dark) { AnyView(stack { EnterCodeView(model: model(knownHost: "100.77.122.82")) }) },
                GalleryScene("pairing-confirm\(suffix)", dark: dark) { AnyView(stack { ConfirmView(model: model(), link: link) }) },
                GalleryScene("pairing-pairing\(suffix)", dark: dark) { AnyView(stack { ConfirmView(model: model(phase: .pairing), link: link) }) },
                GalleryScene("pairing-unreachable\(suffix)", dark: dark) {
                    AnyView(stack { ConfirmView(model: model(phase: .failed(.unreachable)), link: link) })
                },
                GalleryScene("pairing-expired\(suffix)", dark: dark) { AnyView(stack { ConfirmView(model: model(phase: .failed(.expired)), link: link) }) },
                GalleryScene("ended-revoked\(suffix)", dark: dark) { AnyView(ended(.revoked)) },
                GalleryScene("ended-update-mac\(suffix)", dark: dark) { AnyView(ended(.updateMac)) },
                GalleryScene("ended-update-phone\(suffix)", dark: dark) { AnyView(ended(.updatePhone)) },
                GalleryScene("banner-reconnecting\(suffix)", size: CGSize(width: 393, height: 120), dark: dark) {
                    AnyView(banner(offlineFor: 10))
                },
                GalleryScene("banner-unreachable\(suffix)", size: CGSize(width: 393, height: 120), dark: dark) {
                    AnyView(banner(offlineFor: 45))
                },
            ]
        }
    }

    @MainActor
    static func model(phase: PairingModel.Phase = .idle, knownHost: String? = nil) -> PairingModel {
        let model = PairingModel(app: AppModel(), knownHost: knownHost, phoneName: "Mingo's iPhone")
        model.path = []
        model.phase = phase
        return model
    }

    @MainActor
    static func stack<V: View>(@ViewBuilder _ view: () -> V) -> some View {
        NavigationStack { view() }
    }

    @MainActor
    static func host(_ events: [ConnectionEvent], offlineFor: TimeInterval = 0) -> HostModel {
        let credentials = Credentials(host: link.host, deviceToken: "t", deviceId: "d1", macName: link.name, clientId: "ios-gallery")
        let start = Date().addingTimeInterval(-offlineFor)
        let host = HostModel(credentials: credentials, now: { start })
        host.apply(events)
        return host
    }

    @MainActor
    static func ended(_ outcome: CloseOutcome) -> some View {
        EndedView(app: AppModel(), host: host([.connected(Sample.hello), .closed(outcome)]))
    }

    @MainActor
    static func banner(offlineFor: TimeInterval) -> some View {
        VStack {
            ConnectionBanner(host: host([.closed(.retrying(seconds: 2))], offlineFor: offlineFor))
            Spacer()
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .screenBackground()
    }
}
