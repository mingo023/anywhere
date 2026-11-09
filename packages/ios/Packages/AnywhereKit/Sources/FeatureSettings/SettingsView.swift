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
    @State private var forgetting: HostModel?

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
                        Button("Forget", role: .destructive) { forgetting = host }
                    }
                    .contextMenu {
                        Button("Reconnect", systemImage: "arrow.clockwise") { reconnect(host) }
                        Button("Forget", systemImage: "trash", role: .destructive) { forgetting = host }
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
                Text("Paired Macs")
            } footer: {
                if app.hosts.count > 1 { Text("Switch between Macs from the title of Sessions.") }
            }

            NotificationsSection(hosts: app.hosts)

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
        .scrollContentBackground(.hidden)
        .screenBackground()
        .tint(Palette.text.color)
        .navigationTitle("Settings")
        .confirmationDialog(
            "Forget \(forgetting?.name ?? "")?", isPresented: Binding(get: { forgetting != nil }, set: { if !$0 { forgetting = nil } }),
            titleVisibility: .visible, presenting: forgetting
        ) { host in
            Button("Forget", role: .destructive) { app.forget(host.id) }
        } message: { _ in
            Text("This iPhone stops connecting to it. Pair again to bring it back.")
        }
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

/// A Mac in the list: its name and how its link reads, redrawn at the 4 s and 30 s marks of a drop.
@MainActor
struct MacRow: View {
    let host: HostModel

    var body: some View {
        TimelineView(.explicit(host.reachabilityMarks)) { _ in
            let reachability = host.reachability(at: .now)
            VStack(alignment: .leading, spacing: Space.xxs) {
                Text(host.name).font(Typography.body.weight(.semibold)).foregroundStyle(Palette.text.color).lineLimit(1)
                HStack(spacing: Space.xs + 2) {
                    Circle().fill(Self.dot(reachability).color).frame(width: 7, height: 7)
                    Text(MacFacts.status(reachability, version: host.hello?.version))
                        .font(Typography.footnote).foregroundStyle(Palette.text3.color).lineLimit(1)
                }
            }
            .accessibilityElement(children: .combine)
        }
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
