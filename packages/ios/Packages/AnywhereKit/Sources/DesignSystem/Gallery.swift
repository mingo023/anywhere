import Domain
import SwiftUI

/// Scenes for the gallery: the shared pieces features build their states from.
public enum DesignSystemGallery {
    static let mac = "Minh's MacBook Pro"

    @MainActor public static var scenes: [GalleryScene] {
        let pieces: [(String, CGFloat, @MainActor () -> AnyView)] = [
            ("design-toast", 240, {
                AnyView(Color.clear.toast(.constant(Toast("Mentioned in Fix the flaky login test", actionTitle: "Go to chat") {})))
            }),
            ("design-toast-text", 120, { AnyView(Color.clear.toast(.constant(Toast("Copied path")))) }),
            ("design-net-strips", 220, { AnyView(netStrips) }),
            ("design-placeholder-action", 400, {
                AnyView(Placeholder("Can't reach \(mac)", icon: .cloudOff, detail: "Changes show here once it's back.") {})
            }),
            ("design-hit-target", 120, {
                AnyView(HStack(spacing: Space.s) {
                    ForEach([Lucide.close, .arrowUp, .plus, .ellipsis], id: \.self) { glyph in
                        Button {} label: { LucideIcon(glyph, size: 13).hitTarget() }
                            .buttonStyle(.borderless)
                            .background(Palette.fill3.color, in: RoundedRectangle(cornerRadius: Radius.small))
                    }
                }
                .frame(maxWidth: .infinity, maxHeight: .infinity))
            }),
            ("design-icons", 520, { AnyView(icons) }),
            ("42-component-sheet", 852, { AnyView(componentSheet) }),
        ]
        return pieces.flatMap { name, height, piece in
            [true, false].map { dark in
                GalleryScene("\(name)-\(dark ? "dark" : "light")", size: CGSize(width: 393, height: height), dark: dark) {
                    AnyView(piece().frame(maxWidth: .infinity, maxHeight: .infinity).screenBackground().preferredColorScheme(dark ? .dark : .light))
                }
            }
        }
    }

    @MainActor static var netStrips: some View {
        VStack(spacing: Space.s) {
            NetStrip(.reconnecting, NetStrip.reconnecting(mac: "MacBook Pro"))
            NetStrip(.back, "Back online")
            NetStrip(.failed, NetStrip.failed(mac: "MacBook Pro", asOf: nil) + " · showing data from 9:39", action: ("Retry", {}))
            NetStrip(.offline, "Studio Mac Mini is offline · showing data from 10:42")
            Spacer(minLength: 0)
        }
        .padding(Inset.margin)
    }

    /// Every Lucide icon at the list row's size.
    @MainActor static var icons: some View {
        LazyVGrid(columns: Array(repeating: GridItem(.flexible(), spacing: 0), count: 10), spacing: 14) {
            ForEach(Lucide.allCases, id: \.self) { icon in
                LucideIcon(icon, size: 20).foregroundStyle(Palette.text.color)
            }
        }
        .padding(Inset.margin)
        .frame(maxHeight: .infinity, alignment: .top)
    }

    /// The design's component sheet (frame 42), the pieces this module owns.
    @MainActor static var componentSheet: some View {
        @MainActor func row(_ status: Status, _ title: String, last: Bool = false) -> some View {
            HStack(spacing: 12) {
                StatusGlyph(status, size: 17).frame(width: 20)
                Text(title).font(.system(size: 15)).foregroundStyle(Palette.text.color)
                Spacer()
            }
            .frame(height: 44)
            .padding(.horizontal, Inset.margin)
            .rowDivider(status != .needsYou, leading: Inset.textColumn - 4)
        }
        return ScrollView {
            VStack(alignment: .leading, spacing: 0) {
                GroupHeader("Status")
                VStack(spacing: 0) {
                    row(.needsYou, "Needs you (Inbox)")
                    row(.working, "Working")
                    row(.failed, "Failed")
                    row(.done, "Finished · success")
                    row(.idle, "Idle")
                }
                .insetGroup()
                GroupHeader("Buttons")
                HStack(spacing: Space.s) {
                    Button("Primary") {}.buttonStyle(PillButtonStyle(.primary))
                    Button("Secondary") {}.buttonStyle(PillButtonStyle(.secondary))
                    Button("Deny") {}.buttonStyle(PillButtonStyle(.destructive))
                }
                .padding(.horizontal, Inset.margin)
                HStack(spacing: Space.s) {
                    Button("Disabled") {}.buttonStyle(PillButtonStyle(.secondary)).disabled(true)
                    Button("Fix") {}.buttonStyle(RowActionStyle())
                    Button {} label: { LucideIcon(.plus, size: 20) }.buttonStyle(RoundButtonStyle())
                    Button {} label: { LucideIcon(.ellipsis, size: 20) }.buttonStyle(RoundButtonStyle(size: 36))
                }
                .padding(.horizontal, Inset.margin)
                .padding(.top, Space.s)
                GroupHeader("Strip (one per screen)")
                netStrips.padding(.top, -Inset.margin)
                CountedHeader("Needs action", count: 1)
                GlassTabBar(
                    [.init(0, "Inbox", icon: .inbox, badge: 2), .init(1, "Sessions", icon: .sessions), .init(2, "PRs", icon: .pullRequest, badge: 1),
                     .init(3, "Machines", icon: .laptop, dot: true)],
                    selection: .constant(0))
            }
            .padding(.bottom, Space.xl)
        }
    }
}
