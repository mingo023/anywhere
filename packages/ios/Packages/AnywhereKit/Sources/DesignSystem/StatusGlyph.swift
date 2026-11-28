import Domain
import SwiftUI

/// A session's status as the redesign draws it: an ink dot in a ring for Needs you, the orange dots turning for Working,
/// a red alert for Failed, a green check for Done, and a hollow ring for Idle.
@MainActor
public struct StatusGlyph: View {
    let status: Status
    let size: CGFloat

    public init(_ status: Status, size: CGFloat = 17) {
        self.status = status
        self.size = size
    }

    public var body: some View {
        Group {
            switch status {
            case .needsYou:
                Circle().fill(Palette.text.color.opacity(0.16))
                    .frame(width: size * 18 / 17, height: size * 18 / 17)
                    .overlay(Circle().fill(Palette.text.color).frame(width: size * 8 / 17, height: size * 8 / 17))
            case .working:
                DotSpinner(size: size * 13 / 17)
            case .failed:
                LucideIcon(.circleAlert, size: size).foregroundStyle(Palette.failed.color)
            case .done:
                LucideIcon(.circleCheck, size: size).foregroundStyle(Palette.success.color)
            case .idle:
                Circle().strokeBorder(Palette.text4.color, lineWidth: 1.5).frame(width: size * 9 / 17, height: size * 9 / 17)
            }
        }
        .frame(width: size, height: size)
        .accessibilityElement()
        .accessibilityLabel(status.label ?? "Idle")
    }

    public nonisolated static func color(_ status: Status) -> Token {
        switch status {
        case .needsYou: Palette.text
        case .failed: Palette.failed
        case .done: Palette.success
        case .working: Palette.workingDot
        case .idle: Palette.text4
        }
    }
}

/// Working: a 3×3 grid of orange dots lit one after another round the edge, then the middle.
@MainActor
public struct DotSpinner: View {
    let size: CGFloat
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    public init(size: CGFloat = 13) {
        self.size = size
    }

    /// The order the dots light in, by grid index: clockwise from the top-left, the middle last.
    nonisolated static let order = [0, 1, 2, 5, 8, 7, 6, 3, 4]

    /// A dot's opacity `t` seconds into its own 1 s pulse: up from dim to full by 30%, back down by the end.
    nonisolated static func opacity(_ t: Double) -> Double {
        let t = t - t.rounded(.down)
        return t < 0.3 ? 0.22 + 0.78 * t / 0.3 : 1 - 0.78 * (t - 0.3) / 0.7
    }

    public var body: some View {
        TimelineView(.animation(minimumInterval: 1.0 / 20, paused: reduceMotion)) { context in
            let now = reduceMotion ? 0.3 : context.date.timeIntervalSinceReferenceDate
            Canvas { canvas, box in
                let dot = box.width * 3 / 13, gap = (box.width - dot * 3) / 2
                for index in 0..<9 {
                    let delay = Double(Self.order.firstIndex(of: index) ?? 0) * 0.11
                    let rect = CGRect(x: CGFloat(index % 3) * (dot + gap), y: CGFloat(index / 3) * (dot + gap), width: dot, height: dot)
                    canvas.fill(Path(ellipseIn: rect), with: .color(Palette.workingDot.color.opacity(Self.opacity(now - delay))))
                }
            }
        }
        .frame(width: size, height: size)
        .accessibilityHidden(true)
    }
}
