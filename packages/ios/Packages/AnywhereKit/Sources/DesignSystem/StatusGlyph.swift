import Domain
import SwiftUI

/// A session's status, as the desktop's tones draw it: an ink halo for Needs you, a green one for Done, a bold cross for
/// Failed, the gradient spinner for Working and a ring for Idle. `size` is the spinner's and the cross's; a halo runs a
/// little wider, so the box holds it.
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
                Image(systemName: "xmark").font(.system(size: size * 0.86, weight: .heavy)).foregroundStyle(Palette.failed.color)
            case .needsYou, .done:
                ZStack {
                    Circle().fill(Self.halo(status).color)
                    Circle().fill(Self.color(status).color).frame(width: size * 9 / 14, height: size * 9 / 14)
                }
            }
        }
        .frame(width: box, height: box)
        .accessibilityElement()
        .accessibilityLabel(status.label ?? "Idle")
    }

    public nonisolated static func color(_ status: Status) -> Token {
        switch status {
        case .needsYou: Palette.text
        case .failed: Palette.failed
        case .done: Palette.success
        case .working: Palette.accent
        case .idle: Palette.text5
        }
    }

    nonisolated static func halo(_ status: Status) -> Token {
        status == .done ? Palette.successBackground : Palette.accentBackground
    }
}
