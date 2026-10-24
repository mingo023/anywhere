import DesignSystem
import DiffModel
import Highlight
import SwiftUI

/// How code rows are drawn: SF Mono at the footnote size, one fixed height per row.
public enum CodeStyle {
    static let font = Typography.codeSmall
    /// The longest a line is drawn; the Mac cuts at 1024 characters already.
    static let maxChars = 1024

    /// A line number right-aligned in `digits` columns of the monospaced font, and the gap after it.
    nonisolated static func number(_ n: Int?, digits: Int, dark: Bool, into runs: inout LineRuns) {
        let s = n.map(String.init) ?? ""
        runs.append(String(repeating: " ", count: max(digits - s.count, 0)) + s + " ", rgba: Palette.text4.rgba(dark: dark))
    }

    /// A source line: its number, then its coloured code.
    nonisolated static func codeRow(_ text: String, number: Int?, digits: Int, spans: [HighlightSpan], dark: Bool) -> LineRuns {
        var runs = LineRuns()
        if let number {
            self.number(number, digits: digits, dark: dark, into: &runs)
            runs.append(" ")
        }
        runs.code(text, spans: spans, dark: dark)
        return runs
    }

    /// Which numbers a diff row's gutter shows: both in a unified diff, one side's in a side-by-side half.
    public enum Gutter: Hashable, Sendable {
        case both, old, new
    }

    /// A diff row: its numbers, its sign, its coloured code with changed words tinted, and an ellipsis when the Mac
    /// cut the line.
    public nonisolated static func diffRow(_ line: UnifiedLine, gutter: Gutter, digits: Int, spans: [HighlightSpan], dark: Bool) -> LineRuns {
        var runs = LineRuns()
        switch gutter {
        case .both:
            number(line.old, digits: digits, dark: dark, into: &runs)
            number(line.new, digits: digits, dark: dark, into: &runs)
        case .old: number(line.old, digits: digits, dark: dark, into: &runs)
        case .new: number(line.new, digits: digits, dark: dark, into: &runs)
        }
        runs.append(sign(line.kind) + " ", rgba: signColor(line.kind).rgba(dark: dark))
        let word: Token? =
            switch line.kind {
            case .added: Palette.diffAddWord
            case .deleted: Palette.diffDelWord
            default: nil
            }
        runs.code(line.text, spans: spans, words: line.words, word: word?.rgba(dark: dark), dark: dark)
        if line.cut { runs.append(" …", rgba: Palette.text4.rgba(dark: dark)) }
        return runs
    }

    nonisolated static func sign(_ kind: UnifiedLine.Kind) -> String {
        switch kind {
        case .added: "+"
        case .deleted: "−"
        default: " "
        }
    }

    nonisolated static func signColor(_ kind: UnifiedLine.Kind) -> Token {
        switch kind {
        case .added: Palette.diffAddText
        case .deleted: Palette.diffDelText
        default: Palette.text4
        }
    }
}
