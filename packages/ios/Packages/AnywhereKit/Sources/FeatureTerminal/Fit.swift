import CoreGraphics
import CoreText

/// Sizes for a terminal screen: its font's cell, the cols × rows a phone fits, and pinch zoom.
public enum Fit {
    public static let fontSizes: ClosedRange<CGFloat> = 6...28
    public static let defaultFontSize: CGFloat = 11

    /// One cell of the system monospaced font at `size`, as a terminal view lays it out.
    public static func cell(fontSize size: CGFloat) -> CGSize {
        let font = CTFontCreateUIFontForLanguage(.userFixedPitch, size, nil) ?? CTFontCreateWithName("Menlo" as CFString, size, nil)
        var glyph = CGGlyph()
        var w: UniChar = 0x57
        CTFontGetGlyphsForCharacters(font, &w, &glyph, 1)
        var advance = CGSize.zero
        CTFontGetAdvancesForGlyphs(font, .horizontal, &glyph, &advance, 1)
        let height = (CTFontGetAscent(font) + CTFontGetDescent(font) + CTFontGetLeading(font)).rounded(.up)
        return CGSize(width: advance.width, height: height)
    }

    /// The cols × rows that fill `area`, within what a program can still use and pocketd takes.
    public static func grid(_ area: CGSize, cell: CGSize) -> (cols: Int, rows: Int) {
        guard cell.width > 0, cell.height > 0 else { return (80, 24) }
        let cols = Int((area.width / cell.width).rounded(.down))
        let rows = Int((area.height / cell.height).rounded(.down))
        return (min(max(cols, 20), 1000), min(max(rows, 5), 1000))
    }

    /// The font size after a pinch by `scale`, from `start`.
    public static func zoom(_ start: CGFloat, by scale: CGFloat) -> CGFloat {
        min(max((start * scale * 2).rounded() / 2, fontSizes.lowerBound), fontSizes.upperBound)
    }
}
