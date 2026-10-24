import ActivityModel
import DesignSystem
import Foundation
import SwiftUI

/// The Live Activity in each state the gallery renders: the Lock Screen and the Dynamic Island's compact, minimal and expanded forms.
public enum LiveActivityGallery {
    static let mac = MacActivity(mac: "mingos-mbp", macName: "Mingo's MacBook Pro")

    /// The states to review, by name.
    public static func states(now: Date = Date()) -> [(name: String, snapshot: ActivitySnapshot)] {
        let since = { (seconds: Double) in now.timeIntervalSince1970 - seconds }
        let port = ActivityTitles.Entry(title: "Port the commit graph lanes", place: "anywhere · calm-otter")
        let migration = ActivityTitles.Entry(title: "Approve the migration", place: "anywhere · calm-otter")
        let flaky = ActivityTitles.Entry(title: "Fix the flaky timeline test", place: "anywhere · fix-login")
        return [
            ("working", ActivitySnapshot(attributes: mac, state: .init(working: 2, needsYou: 0, unseen: 0,
                top: .init(slot: "a-working", status: "working", sinceUnix: since(754), answerable: false)), isStale: false, entry: port)),
            ("needs-you", ActivitySnapshot(attributes: mac, state: .init(working: 1, needsYou: 1, unseen: 1,
                top: .init(slot: "a-needs", status: "needsYou", sinceUnix: since(180), verb: "Run", subject: "rm -rf build", answerable: true, request: "r-needs")),
                isStale: false, entry: migration)),
            ("failed", ActivitySnapshot(attributes: mac, state: .init(working: 0, needsYou: 0, unseen: 2,
                top: .init(slot: "a-failed", status: "failed", sinceUnix: since(540), answerable: false)), isStale: false, entry: flaky)),
            ("all-done", ActivitySnapshot(attributes: mac, state: .init(working: 0, needsYou: 0, unseen: 3), isStale: false, entry: ActivityTitles.fallback)),
            ("stale", ActivitySnapshot(attributes: mac, state: .init(working: 2, needsYou: 0, unseen: 0,
                top: .init(slot: "a-working", status: "working", sinceUnix: since(1800), answerable: false)), isStale: true, entry: port)),
        ]
    }

    @MainActor public static var scenes: [GalleryScene] {
        states().flatMap { state in
            [false, true].map { dark in
                GalleryScene("live-activity-\(state.name)\(dark ? "-dark" : "")", size: CGSize(width: 393, height: 760), dark: dark) {
                    AnyView(ActivityBoard(snapshot: state.snapshot))
                }
            }
        }
    }
}

/// One state on a Lock Screen-like wallpaper, with the island's three forms under it.
@MainActor
struct ActivityBoard: View {
    let snapshot: ActivitySnapshot
    @Environment(\.colorScheme) private var scheme

    var body: some View {
        VStack(spacing: Space.xl) {
            LockScreenActivity(snapshot)
                .background(.regularMaterial, in: RoundedRectangle(cornerRadius: Radius.sheet, style: .continuous))
                .padding(.horizontal, Space.m)
            VStack(spacing: Space.m) {
                HStack {
                    IslandRegion.compactLeading(snapshot)
                    Spacer(minLength: 126)
                    IslandRegion.compactTrailing(snapshot)
                }
                .padding(.horizontal, Space.m)
                .frame(height: 37)
                .background(.black, in: Capsule())
                .fixedSize()
                IslandRegion.minimal(snapshot)
                    .frame(width: 37, height: 37)
                    .background(.black, in: Circle())
                VStack(spacing: Space.s) {
                    HStack(alignment: .top) {
                        IslandRegion.expandedLeading(snapshot)
                        Spacer()
                        IslandRegion.expandedTrailing(snapshot)
                    }
                    IslandRegion.expandedCenter(snapshot)
                    IslandRegion.expandedBottom(snapshot)
                }
                .padding(Space.l)
                .background(.black, in: RoundedRectangle(cornerRadius: 42, style: .continuous))
                .padding(.horizontal, Space.s)
                .environment(\.colorScheme, .dark)
            }
        }
        .padding(.vertical, Space.xl)
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
        .background(wallpaper.ignoresSafeArea())
    }

    var wallpaper: LinearGradient {
        scheme == .dark
            ? LinearGradient(colors: [Color(white: 0.16), Color(red: 0.1, green: 0.12, blue: 0.2)], startPoint: .top, endPoint: .bottom)
            : LinearGradient(colors: [Color(red: 0.78, green: 0.84, blue: 0.95), Color(red: 0.93, green: 0.88, blue: 0.84)], startPoint: .top, endPoint: .bottom)
    }
}
