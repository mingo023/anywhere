import DesignSystem
import Domain
import Foundation
import Stores
import SwiftUI

/// A session's row as `SessionRow.dc.html` draws it: its status in the icon column, its title and time, and a line under
/// them. The caller adds what sits below the line, and the hairline above.
@MainActor
struct SessionRowFrame<Line: View>: View {
    let status: Status
    let title: String
    var code = false
    /// The whole title, as a command answered from the row must be seen in full.
    var wraps = false
    var pinned = false
    let updatedAt: Int
    var ticking = false
    var bottom: CGFloat = 12
    @ViewBuilder let line: Line

    var body: some View {
        HStack(alignment: .top, spacing: 0) {
            StatusGlyph(status).frame(width: Inset.textColumn, height: 22).padding(.top, 12)
            VStack(alignment: .leading, spacing: Space.xxs) {
                HStack(spacing: Space.s) {
                    Text(title)
                        .font(code ? .system(size: 14, design: .monospaced) : .system(size: 17))
                        .foregroundStyle(Palette.text.color)
                        .lineLimit(wraps ? nil : 1)
                        .frame(maxWidth: .infinity, minHeight: 22, alignment: .leading)
                    if pinned { LucideIcon(.pin, size: 13).foregroundStyle(Palette.text3.color).accessibilityLabel("Pinned") }
                    Elapsed(updatedAt: updatedAt, ticking: ticking)
                }
                line
            }
            .padding(.top, 12)
            .padding(.bottom, bottom)
            .padding(.trailing, Inset.margin)
        }
        .contentShape(Rectangle())
    }
}

/// The line under a row's title: quiet, one line.
extension Text {
    func rowLine() -> some View {
        font(.system(size: 13)).foregroundStyle(Palette.text2.color).lineLimit(1).frame(maxWidth: .infinity, alignment: .leading)
    }
}

/// A session in a worktree's list: its title, and what it asks, is doing or how it ended. It reads only its own
/// session, so an agent's activity redraws only its row.
@MainActor
struct SessionRow: View {
    let session: SessionModel
    /// Where it is, for a search result; else what it asks or is doing.
    var place: String?
    @Environment(\.staleSince) private var stale

    var body: some View {
        let s = session.summary
        let status = session.status
        SessionRowFrame(status: status, title: SessionText.title(s), pinned: s.pinned == true, updatedAt: s.updatedAt, ticking: status == .working && stale == nil) {
            if let line = place ?? SessionText.note(s, status, requests: session.requests) {
                Text(line).rowLine().shimmer(place == nil && status == .working && s.activity != nil && stale == nil)
            }
        }
        .accessibilityElement(children: .combine)
    }
}

/// A working session's time since its status moved, by the second; anyone else's "ago", by the minute.
@MainActor
struct Elapsed: View {
    let updatedAt: Int
    let ticking: Bool

    var body: some View {
        Group {
            if ticking {
                TimelineView(.periodic(from: .now, by: 1)) { context in
                    Text(TimeText.elapsed(seconds: (Self.ms(context.date) - updatedAt) / 1000))
                }
            } else {
                TimelineView(.everyMinute) { _ in
                    Text(Self.sentence(TimeText.ago(ms: updatedAt, now: Self.ms(.now))))
                }
            }
        }
        .font(Typography.digits)
        .foregroundStyle(Palette.text3.color)
        .lineLimit(1)
        .fixedSize()
    }

    static func ms(_ date: Date) -> Int { Int(date.timeIntervalSince1970 * 1000) }

    /// "Yesterday", where the row's time stands alone.
    nonisolated static func sentence(_ text: String) -> String {
        text.prefix(1).uppercased() + text.dropFirst()
    }
}
