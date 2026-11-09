import DesignSystem
import Stores
import SwiftUI
import Wire

extension PrTone {
    nonisolated var text: Token {
        switch self {
        case .success: Palette.successText
        case .failed: Palette.failedText
        case .waiting: Palette.waitingText
        case .merged: Palette.merged
        case .muted: Palette.text4
        case .quiet: Palette.text3
        }
    }

    /// A pill's fill behind `text`; merged takes GitHub's purple at 12%.
    nonisolated var fill: Token {
        switch self {
        case .success: Palette.successBackground
        case .failed: Palette.failedBackground
        case .waiting: Palette.waitingBackground
        case .merged: Token.fixed(0x8250df1f)
        case .muted, .quiet: Palette.fill3
        }
    }
}

/// A capsule in a PR tone: the verdict, a gate, a thread's state.
@MainActor
struct PrPill: View {
    let text: String
    let tone: PrTone
    var systemImage: String?
    var dot = false

    var body: some View {
        HStack(spacing: Space.xs) {
            if dot { Circle().frame(width: 6, height: 6) }
            if let systemImage { Image(systemName: systemImage).imageScale(.small) }
            Text(text).lineLimit(1)
        }
        .font(Typography.captionStrong)
        .foregroundStyle(tone.text.color)
        .padding(.horizontal, Space.s)
        .padding(.vertical, Space.xxs + 1)
        .background(tone.fill.color, in: Capsule())
    }
}

/// The PR's state icon, as GitHub draws it.
@MainActor
struct PrGlyph: View {
    let state: String
    let draft: Bool

    var body: some View {
        let g = PullRequestText.glyph(state: state, draft: draft)
        Image(systemName: g.systemImage)
            .font(.system(.body, weight: .semibold))
            .foregroundStyle(g.tone.text.color)
            .frame(width: 22)
            .accessibilityLabel(state == "OPEN" ? (draft ? "Draft" : "Open") : state.capitalized)
    }
}
