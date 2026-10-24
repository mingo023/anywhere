import SwiftUI

/// A small capsule label: a status, a model, a count.
@MainActor
public struct Pill: View {
    public enum Tone: Hashable, Sendable {
        case neutral, waiting, failed, success, working, accent
    }

    let text: String
    let tone: Tone
    let systemImage: String?

    public init(_ text: String, tone: Tone = .neutral, systemImage: String? = nil) {
        self.text = text
        self.tone = tone
        self.systemImage = systemImage
    }

    public var body: some View {
        HStack(spacing: Space.xs) {
            if let systemImage { Image(systemName: systemImage).imageScale(.small) }
            Text(text).lineLimit(1)
        }
        .font(Typography.captionStrong)
        .foregroundStyle(Self.colors(tone).text.color)
        .padding(.horizontal, Space.s)
        .padding(.vertical, Space.xxs + 1)
        .background(Self.colors(tone).fill.color, in: Capsule())
    }

    static func colors(_ tone: Tone) -> (text: Token, fill: Token) {
        switch tone {
        case .neutral: (Palette.text2, Palette.fill3)
        case .waiting: (Palette.waitingText, Palette.waitingBackground)
        case .failed: (Palette.failedText, Palette.failedBackground)
        case .success: (Palette.successText, Palette.successBackground)
        case .working: (Palette.working, Palette.fill2)
        case .accent: (Palette.accent, Palette.accentBackground)
        }
    }
}
