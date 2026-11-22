import Domain
import SwiftUI

/// A session's status, as the desktop's tones draw it: the gradient spinner for Working, its grid lit amber for Needs you
/// and green for Done, a red cross of it for Failed, and a ring for Idle.
@MainActor
public struct StatusGlyph: View {
    let status: Status
    let size: CGFloat

    public init(_ status: Status, size: CGFloat = 14) {
        self.status = status
        self.size = size
    }

    public var body: some View {
        let box = size * 16 / 14
        Group {
            switch status {
            case .working:
                GradientSpinner(size: size)
            case .idle:
                Circle().strokeBorder(Palette.text5.color, lineWidth: 1.5).padding(size / 14)
            case .failed:
                DotMatrix(DotMatrix.cross, color: Self.color(status), size: size)
            case .needsYou, .done:
                DotMatrix(DotMatrix.full, color: Self.color(status), size: size)
            }
        }
        .frame(width: box, height: box)
        .accessibilityElement()
        .accessibilityLabel(status.label ?? "Idle")
    }

    public nonisolated static func color(_ status: Status) -> Token {
        switch status {
        case .needsYou: Palette.waiting
        case .failed: Palette.failed
        case .done: Palette.success
        case .working: Palette.accent
        case .idle: Palette.text5
        }
    }
}
