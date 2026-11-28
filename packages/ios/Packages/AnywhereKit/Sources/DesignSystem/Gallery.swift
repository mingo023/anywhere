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
            stripKinds
            Spacer(minLength: 0)
        }
        .padding(Inset.margin)
    }

    @MainActor @ViewBuilder static var stripKinds: some View {
        NetStrip(.reconnecting, NetStrip.reconnecting(mac: "MacBook Pro"))
        NetStrip(.back, "Back online")
        NetStrip(.failed, NetStrip.failed(mac: "MacBook Pro", asOf: nil) + " · showing data from 9:39", action: ("Retry", {}))
        NetStrip(.offline, "Studio Mac Mini is offline · showing data from 10:42")
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

    /// The size of the design's component sheet (frame 42): a board of three 380 pt columns, 40 pt apart and from its edges.
    public static let sheetSize = CGSize(width: 1300, height: 1453.5)

    /// The design's component sheet (frame 42), laid out as its board: the pieces this module owns, around the rows that
    /// features own. Each group is an inset group, so a column is 380 pt plus the group's 16 pt margins.
    @MainActor public static func componentSheet(sessions: AnyView, machines: AnyView, prs: AnyView, permission: AnyView) -> some View {
        HStack(alignment: .top, spacing: 8) {
            column {
                sheetHeader("Session row", first: true)
                sessions
                sheetHeader("Machine row")
                machines
                sheetHeader("PR row")
                prs
                // The design's PR row swiped open to Re-run and More; the app's PR rows have no swipe actions.
                Color.clear.frame(height: 91)
                sheetNote("""
                One inset group per section: radius 14, inset 16, hairline in dark only. Rows: 16 pt state icon (red needs action · green open · \
                grey draft · purple merged), repo #number + time, title 16 semibold ≤ 2 lines, one plain meta line (only the checks part coloured) \
                + diffstat. At most one small capsule: Fix or Merge. Re-run, open on GitHub and the rest live in swipe actions and the long-press menu.
                """)
            }
            column {
                sheetHeader("Status", first: true)
                statusLegend
                diffSample
                sheetHeader("Icons")
                iconSizes
                tabStates
                sheetNote("""
                Lucide, the desktop app's set: 24 grid, 2 pt stroke (1.75 in the tab bar), round caps and joins, never filled. Ink by default; \
                colour only for status. Mac and app marks: 17 pt in rows, 34 pt on a 64 pt tile.
                """, line: 18, top: 0)
                sheetHeader("Buttons")
                buttons
                sheetHeader("Pinned bottom button")
                pinnedButton
                sheetHeader("Strip (one per screen)")
                boxed(VStack(spacing: 10) { stripKinds })
                sheetNote("One strip per screen. Reconnecting and failed dim last-known data to 50% and disable answers and writes. Back online dismisses after 2 s.", line: 18, top: 0)
            }
            column {
                sheetHeader("Permission card", first: true)
                permission
                sheetNote("""
                1 Amber dot with 18% ring + tool question
                2 Pause line, Footnote
                3 Command well, SF Mono, radius 8
                4 Deny (red 11%) · Reply (10% fill) · Allow once (ink)
                5 Footer: branch + continue hint. Card replaces the composer.
                """)
                sheetHeader("Tab bar")
                tabBar
                sheetNote("Badges are ink, not red. Machines shows a dot when a Mac is offline.")
            }
        }
        .padding(.horizontal, 24)
        .padding(.top, 40)
        .frame(width: sheetSize.width, height: sheetSize.height, alignment: .top)
    }

    @MainActor private static func column(@ViewBuilder _ content: () -> some View) -> some View {
        VStack(alignment: .leading, spacing: 10, content: content).frame(width: 380 + 2 * Inset.margin)
    }

    @MainActor private static func sheetHeader(_ title: String, first: Bool = false) -> some View {
        GroupHeader(title).padding(.top, first ? -Space.xl : 14 - Space.xl).padding(.bottom, -Space.s)
    }

    @MainActor private static func sheetNote(_ text: String, line: CGFloat = 20, top: CGFloat = 4) -> some View {
        Text(text).font(.system(size: 13)).lineSpacing(line - 16).foregroundStyle(Palette.text2.color)
            .padding(.horizontal, 2 * Inset.margin).padding(.top, top)
            .frame(maxWidth: .infinity, alignment: .leading)
    }

    /// Inset to the column's edge, as a group is.
    @MainActor private static func boxed(_ content: some View) -> some View {
        content.padding(.horizontal, Inset.margin)
    }

    @MainActor static var statusLegend: some View {
        @MainActor func row(_ mark: some View, _ title: String, tint: Token? = nil, _ note: String, link: Bool = false, first: Bool = false) -> some View {
            HStack(spacing: 12) {
                mark.frame(width: 20)
                Text(title).font(.system(size: 15)).foregroundStyle((tint ?? Palette.text).color)
                Spacer()
                Text(note).font(link ? .system(size: 15) : .system(size: 12, design: .monospaced))
                    .foregroundStyle((link ? Palette.link : Palette.text3).color)
            }
            .frame(height: 44)
            .padding(.horizontal, Inset.margin)
            .rowDivider(!first, leading: 0)
        }
        return VStack(spacing: 0) {
            row(StatusGlyph(.needsYou, size: 17), "Needs you (Inbox)", "ink", first: true)
            row(Circle().fill(Palette.waitingDot.color).frame(width: 8, height: 8).background(Circle().fill(Palette.waitingBackground.color).frame(width: 16, height: 16)),
                "Waiting (permission card)", "#FFB224")
            row(StatusGlyph(.working, size: 17), "Working", "orange")
            row(StatusGlyph(.failed, size: 17), "Failed", tint: Palette.failedText, "#E5484D")
            row(StatusGlyph(.done, size: 17), "Finished · success", tint: Palette.successText, "#30A46C")
            row(LucideIcon(.merge, size: 17).foregroundStyle(Palette.merged.color), "Merged", "purple")
            row(Text("M").font(.system(size: 14, weight: .semibold, design: .monospaced)).foregroundStyle(Palette.modified.color), "Modified file", "#FFCA16")
            row(StatusGlyph(.idle, size: 17), "Idle", "Link", link: true)
        }
        .insetGroup()
    }

    @MainActor static var diffSample: some View {
        VStack(alignment: .leading, spacing: 0) {
            Text("+ beforeEach(async () => {").foregroundStyle(Palette.diffAddText.color)
                .frame(maxWidth: .infinity, minHeight: 22, alignment: .leading).padding(.horizontal, 12).background(Palette.diffAddBackground.color)
            Text("− server.listen()").foregroundStyle(Palette.diffDelText.color)
                .frame(maxWidth: .infinity, minHeight: 22, alignment: .leading).padding(.horizontal, 12).background(Palette.diffDelBackground.color)
        }
        .font(.system(size: 12.5, design: .monospaced))
        .clipShape(RoundedRectangle(cornerRadius: Radius.small))
        .padding(.horizontal, Inset.margin)
    }

    @MainActor static var iconSizes: some View {
        let sizes: [(Lucide, CGFloat, String)] = [(.cloudOff, 13, "Inline, strip"), (.fileText, 17, "List row"), (.chevronLeft, 20, "Nav button"), (.messageCircle, 24, "Tab bar")]
        return HStack(spacing: 0) {
            ForEach(sizes, id: \.1) { icon, size, use in
                VStack(spacing: 8) {
                    LucideIcon(icon, size: size).foregroundStyle(Palette.text.color).frame(height: 24)
                    VStack(spacing: 0) {
                        Text("\(Int(size)) pt").font(.system(size: 12, weight: .semibold)).foregroundStyle(Palette.text.color)
                        Text(use).font(.system(size: 11)).foregroundStyle(Palette.text2.color)
                    }
                }
                .frame(maxWidth: .infinity)
            }
        }
        .padding(.vertical, 16).padding(.horizontal, 8)
        .insetGroup()
    }

    /// A tab unselected and selected, from the tab bar itself.
    @MainActor static var tabStates: some View {
        VStack(spacing: 10) {
            GlassTabBar([.init(0, "Inbox", icon: .inbox), .init(1, "Inbox", icon: .inbox)], selection: .constant(1))
                .padding(.horizontal, -21)
                .padding(8)
                .insetGroup()
            HStack(spacing: 0) {
                Text("Unselected · t2").frame(maxWidth: .infinity)
                Text("Selected · tint + pill").frame(maxWidth: .infinity)
            }
            .font(.system(size: 12)).foregroundStyle(Palette.text2.color)
            .padding(.horizontal, 2 * Inset.margin)
        }
    }

    @MainActor static var buttons: some View {
        boxed(VStack(alignment: .leading, spacing: Space.s) {
            HStack(spacing: Space.s) {
                Button("Primary") {}.buttonStyle(PillButtonStyle(.primary))
                Button("Secondary") {}.buttonStyle(PillButtonStyle(.secondary))
                Button("Deny") {}.buttonStyle(PillButtonStyle(.destructive))
            }
            Button("Disabled") {}.buttonStyle(PillButtonStyle(.secondary)).disabled(true)
        })
    }

    @MainActor static var pinnedButton: some View {
        boxed(
            Button {} label: { Text("Send all to Claude").font(.system(size: 17, weight: .semibold)).frame(maxWidth: .infinity) }
                .buttonStyle(PillButtonStyle(.primary, height: 52))
                .padding(.horizontal, Inset.margin).padding(.bottom, 20)
                .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .bottom)
                .frame(height: 110)
                .background(Palette.background.color, in: RoundedRectangle(cornerRadius: Inset.radius))
                .overlay(RoundedRectangle(cornerRadius: Inset.radius).strokeBorder(Palette.separator.color, lineWidth: 0.5)))
    }

    @MainActor static var tabBar: some View {
        boxed(
            GlassTabBar(
                [.init(0, "Inbox", icon: .inbox, badge: 2), .init(1, "Sessions", icon: .sessions), .init(2, "PRs", icon: .pullRequest, badge: 1),
                 .init(3, "Machines", icon: .laptop, dot: true)],
                selection: .constant(0))
                .frame(width: 394)
                .padding(.bottom, 15)
                .frame(width: 380, height: 100, alignment: .bottom)
                .clipped()
                .background(Palette.background.color)
                .clipShape(RoundedRectangle(cornerRadius: Inset.radius))
                .overlay(RoundedRectangle(cornerRadius: Inset.radius).strokeBorder(Palette.separator.color, lineWidth: 0.5)))
    }
}
