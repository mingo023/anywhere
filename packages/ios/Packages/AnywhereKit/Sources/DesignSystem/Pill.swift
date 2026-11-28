import SwiftUI

/// A small capsule label: a status, a model, a count.
@MainActor
public struct Pill: View {
    public enum Tone: Hashable, Sendable {
        case neutral, waiting, ink, failed, success, working, accent
    }

    let text: String
    let tone: Tone
    let icon: Lucide?
    let dot: Bool

    public init(_ text: String, tone: Tone = .neutral, icon: Lucide? = nil, dot: Bool = false) {
        self.text = text
        self.tone = tone
        self.icon = icon
        self.dot = dot
    }

    public var body: some View {
        HStack(spacing: Space.xs) {
            if dot { Circle().frame(width: 6, height: 6) }
            if let icon { LucideIcon(icon, size: 11) }
            Text(text).lineLimit(1)
        }
        .font(Typography.captionStrong)
        .foregroundStyle(Self.colors(tone).text.color)
        .padding(.horizontal, Space.s)
        .padding(.vertical, Space.xxs + 1)
        .background(Self.colors(tone).fill.color, in: Capsule())
    }

    public static func colors(_ tone: Tone) -> (text: Token, fill: Token) {
        switch tone {
        case .neutral: (Palette.text2, Palette.fill2)
        case .waiting: (Palette.waitingText, Palette.waitingBackground)
        case .failed: (Palette.failedText, Palette.failedBackground)
        case .success: (Palette.successText, Palette.successBackground)
        case .ink: (Palette.text, Palette.accentBackground)
        case .working: (Palette.accent, Palette.accentTint)
        case .accent: (Palette.accent, Palette.accentBackground)
        }
    }
}
