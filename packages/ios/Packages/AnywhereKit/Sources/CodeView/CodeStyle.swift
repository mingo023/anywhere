import DesignSystem
import Highlight
import SwiftUI

/// How code rows are drawn: SF Mono at the footnote size, one fixed height per row.
public enum CodeStyle {
    static let font = Typography.codeSmall
    /// The longest a line is drawn; the Mac cuts at 1024 characters already.
    static let maxChars = 1024

    nonisolated static func color(_ kind: TokenKind, dark: Bool) -> Color? {
        guard let rgb = SyntaxPalette.rgb(kind, dark: dark) else { return nil }
        return Color(red: Double(rgb >> 16 & 0xff) / 255, green: Double(rgb >> 8 & 0xff) / 255, blue: Double(rgb & 0xff) / 255)
    }

    /// A line with its syntax colours and changed-word tints; ranges are UTF-16 offsets into `text`.
    public nonisolated static func attributed(_ text: String, spans: [HighlightSpan], words: [Range<Int>] = [], word: Color? = nil, dark: Bool)
        -> AttributedString
    {
        var out = AttributedString(text)
        guard !spans.isEmpty || !words.isEmpty else { return out }
        let utf16 = text.utf16
        func range(_ r: Range<Int>) -> Range<AttributedString.Index>? {
            guard r.lowerBound >= 0, r.upperBound <= utf16.count, r.lowerBound < r.upperBound else { return nil }
            let lo = utf16.index(utf16.startIndex, offsetBy: r.lowerBound)
            let hi = utf16.index(lo, offsetBy: r.count)
            guard let a = AttributedString.Index(lo, within: out), let b = AttributedString.Index(hi, within: out) else { return nil }
            return a..<b
        }
        for s in spans {
            if let c = color(s.kind, dark: dark), let r = range(s.range) { out[r].foregroundColor = c }
        }
        if let word {
            for w in words {
                if let r = range(w) { out[r].backgroundColor = word }
            }
        }
        return out
    }
}

/// A right-aligned line number in the gutter.
@MainActor
struct LineNumber: View {
    let number: Int?
    let width: CGFloat

    var body: some View {
        Text(number.map(String.init) ?? "")
            .font(CodeStyle.font.monospacedDigit())
            .foregroundStyle(Palette.text4.color)
            .frame(width: width, alignment: .trailing)
            .accessibilityHidden(true)
    }
}
