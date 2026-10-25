import Client
import DesignSystem
import Fixtures
import Foundation
import Stores
import SwiftUI
import Wire

public enum SettingsGallery {
    @MainActor public static var scenes: [GalleryScene] {
        [false, true].flatMap { dark in
            let suffix = dark ? "-dark" : ""
            return [
                GalleryScene("settings\(suffix)", dark: dark) {
                    AnyView(NavigationStack { SettingsView(app: sampleApp(), pair: {}) })
                },
                GalleryScene("settings-mac\(suffix)", dark: dark) {
                    let app = sampleApp()
                    return AnyView(NavigationStack { MacSettingsView(app: app, host: app.hosts[0], maxAccess: "edits") })
                },
                GalleryScene("settings-mac-offline\(suffix)", dark: dark) {
                    let app = sampleApp()
                    return AnyView(NavigationStack { MacSettingsView(app: app, host: app.hosts[1]) })
                },
            ]
        }
    }

    /// Three Macs: one online with the sample snapshot, one dropped 12 s ago, one that revoked this phone.
    @MainActor
    public static func sampleApp() -> AppModel {
        let app = AppModel()
        let online = app.add(credentials("d-mbp", "100.77.122.82:4517", Sample.hello.hostname), start: false)
        online.apply([.connected(Sample.hello)] + Sample.snapshot.map(ConnectionEvent.message))

        let studioHello = HelloOk(
            serverId: "studio", hostname: "Studio", protocolVersion: 3, caps: Caps.phone, protocol: .phone,
            scopes: [.observe, .drive, .approve], host: HostState(tailnet: true, keepingAwake: false, terminals: false, upgradeFailed: "0.1.14"), version: "0.1.12")
        let dropped = Date().addingTimeInterval(-12)
        let studio = HostModel(credentials: credentials("d-studio", "studio.tail4a2b.ts.net:4517", "Studio"), now: { dropped })
        studio.apply([.connected(studioHello), .closed(.retrying(seconds: 4))])
        app.add(studio, start: false)

        let mini = app.add(credentials("d-mini", "100.101.4.9:4517", "Mac mini"), start: false)
        mini.apply([.closed(.revoked)])
        return app
    }

    static func credentials(_ id: String, _ host: String, _ name: String) -> Credentials {
        Credentials(host: host, deviceToken: "t", deviceId: id, macName: name, clientId: "ios-gallery")
    }
}
