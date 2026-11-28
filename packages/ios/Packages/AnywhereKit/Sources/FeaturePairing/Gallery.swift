import Client
import DesignSystem
import Fixtures
import Foundation
import Stores
import SwiftUI

public enum PairingGallery {
    static let link = PairingLink(host: "minhs-macbook-pro:4517", code: "q3xYq3xYq3xYq3xYq3xY_-", name: "Minh's MacBook Pro")

    /// The onboarding frames, each in dark and light, then the screens of a Mac whose link ended.
    @MainActor public static var scenes: [GalleryScene] {
        let frames: [(String, @MainActor () -> AnyView)] = [
            ("01-welcome", { AnyView(PairingView(model: model())) }),
            ("02-scan", { AnyView(stack { ScanView(model: model(), allowed: true) }) }),
            ("02b-scan-camera-denied", { AnyView(stack { ScanView(model: model(), allowed: false) }) }),
            ("03-enter-code", { AnyView(stack { EnterCodeView(model: model(knownHost: "minhs-macbook-pro")) }) }),
            ("04-confirm", { AnyView(stack { ConfirmView(model: model(), link: link) }) }),
            ("04b-confirm-pairing", { AnyView(stack { ConfirmView(model: model(phase: .pairing), link: link) }) }),
            ("04c-confirm-failed", { AnyView(stack { ConfirmView(model: model(phase: .failed(.expired)), link: link) }) }),
            ("04d-confirm-tailscale-missing", { AnyView(stack { ConfirmView(model: model(phase: .failed(.unreachable)), link: link) }) }),
        ]
        let onboarding = frames.flatMap { name, view in
            [false, true].map { dark in GalleryScene("\(name)-\(dark ? "dark" : "light")", dark: dark, view: view) }
        }
        let endedScenes = [false, true].flatMap { dark -> [GalleryScene] in
            let suffix = dark ? "-dark" : ""
            return [
                GalleryScene("ended-revoked\(suffix)", dark: dark) { AnyView(ended(.revoked)) },
                GalleryScene("ended-update-mac\(suffix)", dark: dark) { AnyView(ended(.updateMac)) },
                GalleryScene("ended-update-phone\(suffix)", dark: dark) { AnyView(ended(.updatePhone)) },
                GalleryScene("ended-strip\(suffix)", size: CGSize(width: 393, height: 80), dark: dark) { AnyView(strip(.revoked)) },
                GalleryScene("banner-reconnecting\(suffix)", size: CGSize(width: 393, height: 120), dark: dark) {
                    AnyView(banner(offlineFor: 10))
                },
                GalleryScene("banner-unreachable\(suffix)", size: CGSize(width: 393, height: 120), dark: dark) {
                    AnyView(banner(offlineFor: 45))
                },
            ]
        }
        return onboarding + endedScenes
    }

    @MainActor
    static func model(phase: PairingModel.Phase = .idle, knownHost: String? = nil) -> PairingModel {
        let model = PairingModel(app: AppModel(), knownHost: knownHost, phoneName: "Minh's iPhone")
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
    static func strip(_ outcome: CloseOutcome) -> some View {
        VStack {
            EndedStrip(app: AppModel(), host: host([.connected(Sample.hello), .closed(outcome)]))
            Spacer()
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .screenBackground()
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
