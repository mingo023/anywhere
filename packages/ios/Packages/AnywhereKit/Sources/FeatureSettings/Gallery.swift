import Client
import DesignSystem
import Fixtures
import Foundation
import Stores
import SwiftUI
import Wire

public enum SettingsGallery {
    /// The Machines frames and the screens they open, each in dark and light.
    @MainActor public static var scenes: [GalleryScene] {
        let frames: [(String, @MainActor () -> AnyView)] = [
            ("33-machines", { machines(.steady) }),
            ("33b-machines-reconnecting", { machines(.reconnecting) }),
            ("33c-machines-back-online", { machines(.back) }),
            ("33d-machines-reconnect-failed", { machines(.failed) }),
            ("34-machine-connected", { detail(of: \.macbook, .steady) }),
            ("35-machine-offline", { detail(of: \.studio, .steady) }),
            ("35b-machine-version-mismatch", { detail(of: \.macbook, .versions) }),
            ("35c-machine-revoked", { detail(of: \.macbook, .revoked) }),
            ("37-settings-appearance", { AnyView(NavigationStack { AppearanceView() }) }),
            ("38-settings-about", { AnyView(NavigationStack { AboutView(info: info) }) }),
        ]
        return frames.flatMap { name, view in
            [false, true].map { dark in GalleryScene("\(name)-\(dark ? "dark" : "light")", dark: dark, view: view) }
        }
    }

    /// The component sheet's Mac rows (frame 42): one connected, one offline.
    @MainActor public static var sheetMacRows: some View {
        VStack(spacing: 0) {
            MacRow(host: mac("Minh's MacBook Pro", id: "minhs-macbook-pro", version: "0.1.48"), first: true)
            MacRow(host: mac("Studio Mac Mini", id: "studio-mac-mini", version: "0.1.48", closed: (.stopped, at(hour: 10, minute: 42))))
        }
        .fixedSize(horizontal: false, vertical: true)
        .insetGroup()
    }

    /// What befalls the MacBook Pro in the design's Machines frames.
    enum Trouble {
        case steady, reconnecting, back, failed, versions, revoked
    }

    struct Macs {
        let app: AppModel
        let macbook: HostModel
        let studio: HostModel
    }

    @MainActor static let info: [String: Any] = ["CFBundleShortVersionString": "0.1.48", "CFBundleVersion": "212"]

    /// The design's two Macs: a busy MacBook Pro, and a Mac mini that has been offline since 10:42.
    @MainActor
    static func macs(_ trouble: Trouble) -> Macs {
        let app = AppModel()
        let today = at(hour: 10, minute: 42)
        let outcome: (CloseOutcome, Date)? = switch trouble {
        case .steady, .back: nil
        case .reconnecting: (.retrying(seconds: 4), Date().addingTimeInterval(-10))
        case .failed: (.retrying(seconds: 30), Calendar.current.date(byAdding: .day, value: -1, to: at(hour: 9, minute: 39)) ?? today)
        case .versions: (.updateMac, Date().addingTimeInterval(-60))
        case .revoked: (.revoked, Date().addingTimeInterval(-60))
        }
        let macbook = mac("Minh's MacBook Pro", id: "minhs-macbook-pro", version: trouble == .versions ? "0.1.40" : "0.1.48", busy: true, closed: outcome)
        let studio = mac("Studio Mac Mini", id: "studio-mac-mini", version: "0.1.48", closed: (.stopped, today))
        app.add(macbook, start: false)
        app.add(studio, start: false)
        return Macs(app: app, macbook: macbook, studio: studio)
    }

    @MainActor
    static func machines(_ trouble: Trouble) -> AnyView {
        let macs = macs(trouble)
        var view = SettingsView(app: macs.app, pair: {})
        view.info = info
        if trouble == .back { view.backOnline = [macs.macbook.id] }
        return AnyView(NavigationStack { view })
    }

    @MainActor
    static func detail(of mac: KeyPath<Macs, HostModel>, _ trouble: Trouble) -> AnyView {
        let macs = macs(trouble)
        return AnyView(NavigationStack { MacSettingsView(app: macs.app, host: macs[keyPath: mac], pair: {}) })
    }

    /// A Mac that said hello, with three sessions if `busy`: two working and one that needs you.
    @MainActor
    static func mac(_ name: String, id: String, version: String, busy: Bool = false, closed: (CloseOutcome, Date)? = nil) -> HostModel {
        let hello = HelloOk(
            serverId: id, hostname: name, protocolVersion: 3, caps: Caps.phone, protocol: .phone, scopes: [.observe, .drive, .approve, .spawn],
            host: HostState(tailnet: true, keepingAwake: true, terminals: true), version: version)
        let wanted: Set = ["a-needs", "a-working", "a-compacting"]
        let agents = busy ? [ConnectionEvent.message(.agentList(AgentList(agents: Sample.agents.filter { wanted.contains($0.id) })))] : []
        let since = closed?.1 ?? Date()
        let host = HostModel(credentials: credentials(id, "\(id).tail3c2e.ts.net:4517", name), now: { since })
        host.apply([.connected(hello)] + agents + (closed.map { [ConnectionEvent.closed($0.0)] } ?? []))
        return host
    }

    private static func at(hour: Int, minute: Int) -> Date {
        Calendar.current.date(bySettingHour: hour, minute: minute, second: 0, of: Date()) ?? Date()
    }

    /// Three Macs: one online with the sample snapshot, one dropped 12 s ago, one that revoked this phone.
    @MainActor
    public static func sampleApp() -> AppModel {
        let app = AppModel()
        var hello = Sample.hello
        hello.hostname = "Minh's MacBook Pro"
        hello.version = "0.1.48"
        let online = app.add(credentials("d-mbp", "100.77.122.82:4517", hello.hostname), start: false)
        online.apply([.connected(hello)] + Sample.snapshot.map(ConnectionEvent.message))

        let studioHello = HelloOk(
            serverId: "studio", hostname: "Studio Mac Mini", protocolVersion: 3, caps: Caps.phone, protocol: .phone,
            scopes: [.observe, .drive, .approve], host: HostState(tailnet: true, keepingAwake: false, terminals: false, upgradeFailed: "0.1.14"), version: "0.1.48")
        let dropped = Date().addingTimeInterval(-12)
        let studio = HostModel(credentials: credentials("d-studio", "studio.tail4a2b.ts.net:4517", "Studio Mac Mini"), now: { dropped })
        studio.apply([.connected(studioHello), .closed(.retrying(seconds: 4))])
        app.add(studio, start: false)

        let old = app.add(credentials("d-old", "100.101.4.9:4517", "Old iMac"), start: false)
        old.apply([.closed(.revoked)])
        return app
    }

    static func credentials(_ id: String, _ host: String, _ name: String) -> Credentials {
        Credentials(host: host, deviceToken: "t", deviceId: id, macName: name, clientId: "ios-gallery")
    }
}
