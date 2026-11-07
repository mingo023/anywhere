import CoreGraphics
import CoreText
import DesignSystem

#if canImport(UIKit)
    import UIKit
#else
    import AppKit
#endif

/// Sizes for a terminal screen: its font's cell, the cols × rows a phone fits, and pinch zoom.
public enum Fit {
    public static let fontSizes: ClosedRange<CGFloat> = 6...28
    public static let defaultFontSize: CGFloat = 11

    /// SF Mono, as the phone's terminal view draws it.
    public static func font(size: CGFloat) -> CTFont {
        #if canImport(UIKit)
            UIFont.monospacedSystemFont(ofSize: size, weight: .regular) as CTFont
        #else
            NSFont.monospacedSystemFont(ofSize: size, weight: .regular) as CTFont
        #endif
    }

    /// One cell of `font(size:)`, as a terminal view lays it out.
    public static func cell(fontSize size: CGFloat) -> CGSize {
        let font = font(size: size)
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

    /// The cols × rows a terminal screen of `area` shows without scrolling, inside its padding.
    public static func screen(_ area: CGSize, cell: CGSize) -> (cols: Int, rows: Int) {
        grid(CGSize(width: area.width - 2 * inset, height: area.height - 2 * inset), cell: cell)
    }

    /// The cols × rows a terminal created on a phone opens at: its screen takes the terminal list's `area`, less the key bar.
    public static func newTerminal(in area: CGSize, fontSize: CGFloat) -> (cols: Int, rows: Int) {
        screen(CGSize(width: area.width, height: area.height - KeyBar.height), cell: cell(fontSize: fontSize))
    }

    /// Around the screen, inside its scroll view.
    public static let inset = Space.xs

    /// The font size after a pinch by `scale`, from `start`.
    public static func zoom(_ start: CGFloat, by scale: CGFloat) -> CGFloat {
        min(max((start * scale * 2).rounded() / 2, fontSizes.lowerBound), fontSizes.upperBound)
    }
}
