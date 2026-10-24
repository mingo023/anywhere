import DesignSystem
import Domain
import Stores
import SwiftUI

/// Paired Macs, notifications, appearance and about.
@MainActor
public struct SettingsView: View {
    let app: AppModel
    let pair: (@MainActor () -> Void)?
    @AppStorage(Appearance.key) private var appearance = Appearance.system

    /// `pair` shows the pairing flow; the shell owns it because features don't import each other.
    public init(app: AppModel, pair: (@MainActor () -> Void)? = nil) {
        self.app = app
        self.pair = pair
    }

    public var body: some View {
        Form {
            Section {
                ForEach(app.hosts) { host in
                    NavigationLink {
                        MacSettingsView(app: app, host: host)
                    } label: {
                        MacRow(host: host)
                    }
                    .swipeActions(edge: .trailing) {
                        Button("Forget", role: .destructive) { app.forget(host.id) }
                    }
                    .contextMenu {
                        Button("Reconnect", systemImage: "arrow.clockwise") { reconnect(host) }
                        Button("Forget", systemImage: "trash", role: .destructive) { app.forget(host.id) }
                    }
                }
                if let pair {
                    Button {
                        Haptics.tap()
                        pair()
                    } label: {
                        Label("Pair another Mac", systemImage: "plus")
                    }
                }
            } header: {
                Text("Macs")
            } footer: {
                if app.hosts.count > 1 { Text("Switch between Macs from the title of Sessions.") }
            }

            // INTEGRATION (push slice): replace this placeholder with `NotificationsSection(app: app)` once
            // FeatureSettings/NotificationsSection.swift lands.
            NotificationsPlaceholder()

            Section("Appearance") {
                Picker("Appearance", selection: $appearance) {
                    ForEach(Appearance.allCases) { Text($0.label).tag($0) }
                }
                .pickerStyle(.segmented)
                .labelsHidden()
            }

            Section {
                LabeledContent("Version", value: Self.version)
                LabeledContent("Protocol", value: "3")
                Link(destination: URL(string: "https://tailscale.com/kb/1017/install")!) {
                    Label("Setting up Tailscale", systemImage: "network")
                }
            } header: {
                Text("About")
            } footer: {
                Text("Anywhere talks to your Macs over your own tailnet. Nothing passes through a server but encrypted notifications.")
            }
        }
        .formStyle(.grouped)
        .navigationTitle("Settings")
    }

    private func reconnect(_ host: HostModel) {
        if host.reachability(at: .now).isFinal { app.restart(host.id) } else { host.scene(active: true) }
    }

    static var version: String {
        let info = Bundle.main.infoDictionary
        guard let short = info?["CFBundleShortVersionString"] as? String else { return "Development" }
        return (info?["CFBundleVersion"] as? String).map { "\(short) (\($0))" } ?? short
    }
}

/// Where the push slice's notification preferences go; until then, a way to the system's notification settings.
@MainActor
struct NotificationsPlaceholder: View {
    var body: some View {
        Section {
            #if os(iOS)
                Link(destination: URL(string: UIApplication.openNotificationSettingsURLString)!) {
                    Label("Notifications", systemImage: "bell.badge")
                }
            #else
                Label("Notifications", systemImage: "bell.badge")
            #endif
        } header: {
            Text("Notifications")
        } footer: {
            Text("Anywhere tells you when a session needs you, finishes or fails.")
        }
    }
}

/// A Mac in the list: its name and how its link reads, redrawn at the 4 s and 30 s marks of a drop.
@MainActor
struct MacRow: View {
    let host: HostModel

    var body: some View {
        TimelineView(.explicit(marks)) { _ in
            let reachability = host.reachability(at: .now)
            HStack(spacing: Space.m) {
                Image(systemName: "laptopcomputer")
                    .font(.system(.title3))
                    .foregroundStyle(Palette.text2.color)
                    .overlay(alignment: .bottomTrailing) {
                        Circle().fill(Self.dot(reachability).color).frame(width: 9, height: 9)
                            .overlay(Circle().stroke(Palette.surface.color, lineWidth: 2))
                            .offset(x: 3, y: 3)
                    }
                    .frame(width: 32)
                VStack(alignment: .leading, spacing: Space.xxs) {
                    Text(host.name).font(Typography.body).foregroundStyle(Palette.text.color).lineLimit(1)
                    Text(MacFacts.status(reachability, version: host.hello?.version))
                        .font(Typography.footnote).foregroundStyle(Palette.text3.color).lineLimit(1)
                }
            }
            .accessibilityElement(children: .combine)
        }
    }

    private var marks: [Date] {
        guard let since = host.offlineSince else { return [] }
        return [since, since + Reachability.reconnectingAfter, since + Reachability.unreachableAfter]
    }

    nonisolated static func dot(_ reachability: Reachability) -> Token {
        switch reachability {
        case .online: Palette.success
        case .connecting, .reconnecting: Palette.waitingDot
        case .unreachable, .revoked, .updateMac, .updatePhone: Palette.failed
        case .stopped: Palette.text4
        }
    }
}
