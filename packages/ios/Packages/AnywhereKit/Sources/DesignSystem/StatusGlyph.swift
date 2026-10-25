import Domain
import SwiftUI

/// A session's status as a dot: amber Needs you, red Failed, green Done, the dot spinner for Working, a ring for Idle.
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
                DotSpinner(size: size)
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

/// The desktop's braille dot spinner, a frame every tenth of a second.
@MainActor
public struct DotSpinner: View {
    static let frames = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"]

    let size: CGFloat
    let tint: Token

    public init(size: CGFloat, tint: Token = Palette.working) {
        self.size = size
        self.tint = tint
    }

    public var body: some View {
        TimelineView(.periodic(from: .now, by: 0.1)) { context in
            let frame = Int(context.date.timeIntervalSinceReferenceDate * 10) % Self.frames.count
            // A braille glyph's dots cover a fraction of its em box.
            Text(Self.frames[frame]).font(.system(size: size * 1.6)).foregroundStyle(tint.color).fixedSize()
        }
        .frame(width: size, height: size)
    }
}
