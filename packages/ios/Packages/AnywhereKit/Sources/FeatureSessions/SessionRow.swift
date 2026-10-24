import DesignSystem
import Domain
import Foundation
import Stores
import SwiftUI

/// A session's row: status, title, what it is doing, and how long ago its status last moved. It reads only its own
/// session, so an agent's activity redraws only its row.
@MainActor
struct SessionRow: View {
    let session: SessionModel

    var body: some View {
        let s = session.summary
        let status = session.status
        HStack(alignment: .top, spacing: Space.m) {
            StatusGlyph(status).padding(.top, 5)
            VStack(alignment: .leading, spacing: Space.xxs) {
                HStack(alignment: .firstTextBaseline, spacing: Space.xs) {
                    if s.pinned == true {
                        Image(systemName: "pin.fill").font(.caption2).foregroundStyle(Palette.text4.color).accessibilityLabel("Pinned")
                    }
                    Text(SessionText.title(s))
                        .font(status.wantsLook ? Typography.body.weight(.semibold) : Typography.body)
                        .foregroundStyle(Palette.text.color)
                        .lineLimit(1)
                    Spacer(minLength: Space.s)
                    Elapsed(updatedAt: s.updatedAt, ticking: status == .working)
                }
                if let detail = SessionText.detail(s, status) {
                    Text(detail).font(Typography.footnote).foregroundStyle(Palette.text3.color).lineLimit(1)
                        .shimmer(status == .working && s.activity != nil)
                }
            }
        }
        .padding(.vertical, Space.xxs)
        .contentShape(Rectangle())
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
                TimelineView(.everyMinute) { context in
                    Text(TimeText.ago(ms: updatedAt, now: Self.ms(context.date)))
                }
            }
        }
        .font(Typography.digits)
        .foregroundStyle(Palette.text4.color)
    }

    static func ms(_ date: Date) -> Int { Int(date.timeIntervalSince1970 * 1000) }
}
