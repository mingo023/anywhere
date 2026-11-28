import DesignSystem
import Domain
import Stores
import SwiftUI

/// The Machines tab: the paired Macs, then notifications, appearance and about.
@MainActor
public struct SettingsView: View {
    let app: AppModel
    let pair: (@MainActor () -> Void)?
    /// Hosts whose strip starts at "Back online", for the gallery.
    var backOnline: Set<String> = []
    var info = Bundle.main.infoDictionary
    @AppStorage(Appearance.key) private var appearance = Appearance.system
    @State private var forgetting: HostModel?

    /// `pair` shows the pairing flow; the shell owns it because features don't import each other.
    public init(app: AppModel, pair: (@MainActor () -> Void)? = nil) {
        self.app = app
        self.pair = pair
    }

    public var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 0) {
                HStack {
                    Spacer()
                    if let pair {
                        Button {
                            Haptics.tap()
                            pair()
                        } label: {
                            LucideIcon(.plus, size: 20)
                        }
                        .buttonStyle(RoundButtonStyle())
                        .accessibilityLabel("Pair a Mac")
                    }
                }
                .padding(.horizontal, Inset.margin)
                .frame(height: HitTarget.minimum)
                ScreenTitle("Machines")
                ForEach(app.hosts) { MacStrip(host: $0, back: backOnline.contains($0.id)) }
                if !app.hosts.isEmpty { macs }
                GroupHeader("Settings")
                VStack(spacing: 0) {
                    NavigationLink { NotificationsView() } label: { NavRow(icon: .bell, title: "Notifications", first: true) }
                    NavigationLink { AppearanceView() } label: { NavRow(icon: .sunMoon, title: "Appearance", value: appearance.label) }
                    NavigationLink { AboutView(info: info) } label: { NavRow(icon: .info, title: "About", value: AppVersion.short(info)) }
                }
                .buttonStyle(.plain)
                .insetGroup()
            }
            .padding(.bottom, Space.xl)
        }
        .screenBackground()
        .hidesNavigationBar()
        .confirmationDialog(
            "Forget \(forgetting?.name ?? "")?", isPresented: Binding(get: { forgetting != nil }, set: { if !$0 { forgetting = nil } }),
            titleVisibility: .visible, presenting: forgetting
        ) { host in
            Button("Forget", role: .destructive) { app.forget(host.id) }
        } message: { _ in
            Text("This iPhone stops connecting to it. Pair again to bring it back.")
        }
    }

    private var macs: some View {
        VStack(spacing: 0) {
            ForEach(Array(app.hosts.enumerated()), id: \.element.id) { index, host in
                NavigationLink {
                    MacSettingsView(app: app, host: host, pair: pair)
                } label: {
                    MacRow(host: host, first: index == 0)
                }
                .buttonStyle(.plain)
                .contextMenu {
                    Button { reconnect(host) } label: { Label("Reconnect", lucide: .rotateCw) }
                    Button(role: .destructive) { forgetting = host } label: { Label("Forget", lucide: .trash) }
                }
            }
        }
        .insetGroup()
        .padding(.top, Space.l)
    }

    private func reconnect(_ host: HostModel) {
        if host.reachability(at: .now).isFinal { app.restart(host.id) } else { host.scene(active: true) }
    }
}

/// A Mac in the list: its name, how its link reads, and what its sessions want, redrawn at the 4 s and 30 s marks of a drop.
@MainActor
struct MacRow: View {
    let host: HostModel
    var first = false

    var body: some View {
        TimelineView(.explicit(host.reachabilityMarks)) { _ in
            let reachability = host.reachability(at: .now)
            let sessions = host.sessions.sessions.values
            let activity = MacFacts.activity(working: sessions.count { $0.status == .working }, needsYou: sessions.count { $0.status == .needsYou })
            HStack(alignment: .top, spacing: 0) {
                LucideIcon(MacFacts.icon(forName: host.name), size: 17).foregroundStyle(Palette.text2.color)
                    .frame(width: Inset.textColumn, height: 22)
                    .padding(.top, Space.m)
                VStack(alignment: .leading, spacing: Space.xxs) {
                    Text(host.name).font(.system(size: 17)).foregroundStyle(Palette.text.color).lineLimit(1).frame(minHeight: 22)
                    HStack(spacing: 6) {
                        Circle().fill(MacFacts.dot(reachability).color).frame(width: 7, height: 7)
                        Text(MacFacts.status(reachability, version: host.hello?.version)).lineLimit(1)
                    }
                    .frame(minHeight: 18)
                    if let activity { Text(activity).lineLimit(1).frame(minHeight: 18) }
                }
                .font(.system(size: 13))
                .foregroundStyle(Palette.text2.color)
                .padding(.vertical, Space.m)
                Spacer(minLength: Space.s)
                LucideIcon(.chevronRight, size: 17).foregroundStyle(Palette.text4.color)
                    .frame(maxHeight: .infinity)
                    .padding(.trailing, Space.m)
            }
            .contentShape(Rectangle())
            .rowDivider(!first)
            .accessibilityElement(children: .combine)
        }
    }
}

extension View {
    /// The screen leads with its own large title and corner button.
    @ViewBuilder
    func hidesNavigationBar() -> some View {
        #if os(iOS)
            toolbar(.hidden, for: .navigationBar)
        #else
            self
        #endif
    }

    /// The system's inline bar over a pushed screen: its back button, and the title left to the screen's own.
    @ViewBuilder
    func pushedScreen(_ title: String) -> some View {
        #if os(iOS)
            navigationTitle(title).navigationBarTitleDisplayMode(.inline).toolbar { NoBarTitle() }
        #else
            navigationTitle(title).toolbar { NoBarTitle() }
        #endif
    }
}
