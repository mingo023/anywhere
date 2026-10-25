import SwiftUI

/// Sizes of one markdown style, in points, scaled for the reader's text size
/// (desktop `pocket/chat/markdown.rs style()`: body 14/24, H1 19/27, H2 16/24, H3 15/22, H4–6 14/22).
struct Metrics: Equatable, Sendable {
    var body: CGFloat
    var line: CGFloat
    /// Size and line height for heading levels 1, 2, 3 and 4–6.
    var headings: [(size: CGFloat, line: CGFloat)]
    /// Between blocks.
    var gap: CGFloat
    /// Between list items.
    var itemGap: CGFloat
    /// How far a list item's text sits from its marker's edge.
    var indent: CGFloat
    var codeSize: CGFloat
    var codeLine: CGFloat

    static func == (a: Metrics, b: Metrics) -> Bool {
        a.body == b.body && a.line == b.line && a.gap == b.gap && a.codeSize == b.codeSize && a.headings.map(\.size) == b.headings.map(\.size)
    }

    nonisolated static func of(_ style: MarkdownStyle, scale: CGFloat = 1) -> Metrics {
        let m: Metrics
        switch style {
        case .chat:
            m = Metrics(body: 14, line: 24, headings: [(19, 27), (16, 24), (15, 22), (14, 22)], gap: 16, itemGap: 8, indent: 14 * 1.9, codeSize: 12.5, codeLine: 18)
        case .compact:
            m = Metrics(body: 13, line: 20, headings: [(16, 22), (14.5, 21), (13.5, 20), (13, 20)], gap: 10, itemGap: 6, indent: 13 * 1.9, codeSize: 12, codeLine: 17)
        case .document:
            m = Metrics(body: 15, line: 25, headings: [(24, 32), (20, 28), (17, 24), (15, 23)], gap: 16, itemGap: 8, indent: 15 * 1.9, codeSize: 13, codeLine: 19)
        }
        guard scale != 1 else { return m }
        return Metrics(
            body: m.body * scale, line: m.line * scale, headings: m.headings.map { ($0.size * scale, $0.line * scale) }, gap: m.gap * scale,
            itemGap: m.itemGap * scale, indent: m.indent * scale, codeSize: m.codeSize * scale, codeLine: m.codeLine * scale)
    }

    func heading(_ level: Int) -> (size: CGFloat, line: CGFloat) { headings[min(max(level, 1), 4) - 1] }

    /// The space to add between lines of `size` text for it to sit on `line`-high lines.
    nonisolated static func spacing(size: CGFloat, line: CGFloat) -> CGFloat { max(0, line - size * 1.19) }

    /// How much bigger the reader's text size makes text, as UIKit's body style grows (17 pt at Large).
    nonisolated static func scale(_ size: DynamicTypeSize) -> CGFloat {
        switch size {
        case .xSmall: 14 / 17
        case .small: 15 / 17
        case .medium: 16 / 17
        case .large: 1
        case .xLarge: 19 / 17
        case .xxLarge: 21 / 17
        case .xxxLarge: 23 / 17
        case .accessibility1: 28 / 17
        case .accessibility2: 33 / 17
        case .accessibility3: 40 / 17
        case .accessibility4: 47 / 17
        case .accessibility5: 53 / 17
        @unknown default: 1
        }
    }
}
