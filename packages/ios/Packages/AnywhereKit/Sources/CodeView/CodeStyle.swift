import DesignSystem
import DiffModel
import Highlight
import SwiftUI

/// How code rows are drawn: SF Mono at the footnote size, one fixed height per row.
public enum CodeStyle {
    static let font = Typography.codeSmall
    /// The longest a line is drawn; the Mac cuts at 1024 characters already.
    public static let maxChars = 1024
    /// Points a reader can add to or take from the Dynamic Type size of code.
    public static let textSizes = -4...8

    static let match = Palette.waitingBackground
    static let currentMatch = Token.fixed(0xffb22480)

    /// A line number right-aligned in `digits` columns of the monospaced font, and the gap after it.
    nonisolated static func number(_ n: Int?, digits: Int, dark: Bool, into runs: inout LineRuns) {
        let s = n.map(String.init) ?? ""
        runs.append(String(repeating: " ", count: max(digits - s.count, 0)) + s + " ", rgba: Palette.text4.rgba(dark: dark))
    }

    /// Where a source row's code starts in its string: after the gutter, when there is one.
    nonisolated static func codeStart(digits: Int?) -> Int { digits.map { $0 + 2 } ?? 0 }

    /// A source row: its number (blank on a wrapped line's later rows), then its coloured code, or only `part` of it,
    /// and an ellipsis when the line goes on past what's drawn.
    nonisolated static func codeRow(
        _ text: String, number: Int?, digits: Int?, spans: [HighlightSpan], dark: Bool, part: Range<Int>? = nil, cut: Bool = false
    ) -> LineRuns {
        var runs = LineRuns()
        if let digits {
            self.number(number, digits: digits, dark: dark, into: &runs)
            runs.append(" ")
        }
        runs.code(text, spans: spans, dark: dark, part: part)
        if cut { runs.append(" …", rgba: Palette.text4.rgba(dark: dark)) }
        return runs
    }

    /// Which numbers a diff row's gutter shows: both in a unified diff, one side's in a side-by-side half, or `one`
    /// (the new line's, or the old one's for a deletion) when a narrow screen wraps.
    public enum Gutter: Hashable, Sendable {
        case both, old, new, one
    }

    /// Where a diff row's code starts in its string, after its numbers and sign.
    nonisolated static func diffCodeStart(_ gutter: Gutter, digits: Int) -> Int {
        (gutter == .both ? 2 : 1) * (digits + 1) + 2
    }

    /// A diff row: its numbers, its sign, its coloured code with changed words tinted, and an ellipsis when the Mac
    /// cut the line. With `part`, that piece of a wrapped line; pieces after the first leave the gutter blank.
    public nonisolated static func diffRow(
        _ line: UnifiedLine, gutter: Gutter, digits: Int, spans: [HighlightSpan], dark: Bool, part: Range<Int>? = nil
    ) -> LineRuns {
        var runs = LineRuns()
        diffGutter(line, gutter: gutter, digits: digits, dark: dark, first: (part?.lowerBound ?? 0) == 0, into: &runs)
        let word: Token? =
            switch line.kind {
            case .added: Palette.diffAddWord
            case .deleted: Palette.diffDelWord
            default: nil
            }
        runs.code(line.text, spans: spans, words: line.words, word: word?.rgba(dark: dark), dark: dark, part: part)
        if line.cut, (part?.upperBound ?? .max) >= line.text.utf16.count { runs.append(" …", rgba: Palette.text4.rgba(dark: dark)) }
        return runs
    }

    /// A diff row's numbers and sign, the `diffCodeStart` columns before its code; blank but for spacing when the row
    /// isn't a line's `first`.
    nonisolated static func diffGutter(
        _ line: UnifiedLine, gutter: Gutter, digits: Int, dark: Bool, first: Bool = true, into runs: inout LineRuns
    ) {
        func num(_ n: Int?) { number(first ? n : nil, digits: digits, dark: dark, into: &runs) }
        switch gutter {
        case .both:
            num(line.old)
            num(line.new)
        case .old: num(line.old)
        case .new: num(line.new)
        case .one: num(line.new ?? line.old)
        }
        runs.append((first ? sign(line.kind) : " ") + " ", rgba: signColor(line.kind).rgba(dark: dark))
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
