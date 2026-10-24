import Domain
import SwiftUI

/// A session's status as a dot: amber Needs you, red Failed, green Done, a turning arc for Working, a ring for Idle.
@MainActor
public struct StatusGlyph: View {
    let status: Status
    let size: CGFloat

    public init(_ status: Status, size: CGFloat = 10) {
        self.status = status
        self.size = size
    }

    public var body: some View {
        Group {
            switch status {
            case .working:
                Spinner(size: size)
            case .idle:
                Circle().strokeBorder(Palette.text5.color, lineWidth: 1.5)
            default:
                Circle().fill(Self.color(status).color)
            }
        }
        .frame(width: size, height: size)
        .accessibilityLabel(status.label ?? "Idle")
    }

    public nonisolated static func color(_ status: Status) -> Token {
        switch status {
        case .needsYou: Palette.waitingDot
        case .failed: Palette.failed
        case .done: Palette.success
        case .working: Palette.working
        case .idle: Palette.text5
        }
    }
}

/// A ring with an arc turning once a second, redrawn at most 20 times a second.
@MainActor
struct Spinner: View {
    let size: CGFloat

    var body: some View {
        TimelineView(.animation(minimumInterval: 1.0 / 20)) { context in
            let turn = context.date.timeIntervalSinceReferenceDate.truncatingRemainder(dividingBy: 1)
            ZStack {
                Circle().stroke(Palette.fill4.color, lineWidth: 1.5)
                Circle().trim(from: 0, to: 0.3).stroke(Palette.working.color, style: StrokeStyle(lineWidth: 1.5, lineCap: .round))
                    .rotationEffect(.degrees(turn * 360))
            }
        }
    }
}
