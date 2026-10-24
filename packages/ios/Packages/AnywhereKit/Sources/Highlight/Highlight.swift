import Foundation

/// A token's class, mapped to the desktop's Xcode palette.
public enum TokenKind: UInt8, Sendable, CaseIterable {
    case plain, keyword, string, comment, number, type, title, function, property, preproc, attribute, url
}

/// A coloured run; `range` is in UTF-16 offsets of the highlighted text.
public struct HighlightSpan: Sendable, Equatable {
    public var range: Range<Int>
    public var kind: TokenKind
    public init(range: Range<Int>, kind: TokenKind) {
        self.range = range
        self.kind = kind
    }
}

public enum Highlighter {
    /// Spans of `code` in `language` (a protocol.md language id or a fence label), sorted, non-overlapping.
    /// Pure and thread-safe; call it off the main actor.
    public static func spans(_ code: String, language: String) -> [HighlightSpan] { [] }

    /// The language id for a file path or fence label ("sh" → "bash"), "text" when unknown.
    public static func language(for pathOrLabel: String) -> String { "text" }
}

/// sRGB colour as 0xRRGGBB.
public enum SyntaxPalette {
    public static func rgb(_ kind: TokenKind, dark: Bool) -> UInt32? { nil }
}
