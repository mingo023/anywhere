import CoreText
import DesignSystem
import Highlight
import SwiftUI

#if os(iOS)
    import UIKit
#else
    import AppKit
#endif

/// A row's text as plain runs: the string, the colour of each run and the tint behind changed words, in UTF-16 offsets.
public struct LineRuns: Equatable, Sendable {
    var string = ""
    var colors: [Run] = []
    var tints: [Run] = []
    private var length = 0

    struct Run: Equatable, Sendable {
        let range: Range<Int>
        let rgba: UInt32
    }

    /// Text in one colour, such as a line number or a diff's sign.
    mutating func append(_ s: String, rgba: UInt32? = nil) {
        let n = s.utf16.count
        if let rgba, n > 0 { colors.append(Run(range: length..<length + n, rgba: rgba)) }
        string += s
        length += n
    }

    /// A line of code with its syntax colours and changed-word tints; ranges outside the line are dropped.
    mutating func code(_ text: String, spans: [HighlightSpan], words: [Range<Int>] = [], word: UInt32? = nil, dark: Bool) {
        let n = text.utf16.count
        let at = length
        func inside(_ r: Range<Int>) -> Range<Int>? {
            r.lowerBound >= 0 && r.upperBound <= n && r.lowerBound < r.upperBound ? (at + r.lowerBound)..<(at + r.upperBound) : nil
        }
        for s in spans {
            if let rgb = SyntaxPalette.rgb(s.kind, dark: dark), let r = inside(s.range) { colors.append(Run(range: r, rgba: rgb << 8 | 0xff)) }
        }
        if let word {
            for w in words {
                if let r = inside(w) { tints.append(Run(range: r, rgba: word)) }
            }
        }
        string += text
        length += n
    }
}

/// A line laid out once by CoreText, drawn as is each time its row comes on screen.
public final class DrawnText: @unchecked Sendable {
    let line: CTLine
    let width: CGFloat
    private let ascent: CGFloat
    private let descent: CGFloat
    private let tints: [(from: CGFloat, to: CGFloat, color: CGColor)]

    public init(_ runs: LineRuns, size: CGFloat, color rgba: UInt32) {
        let font = Self.font(size)
        let text = NSMutableAttributedString(
            string: runs.string, attributes: [Self.fontKey: font, Self.colorKey: Self.cgColor(rgba)])
        for run in runs.colors {
            text.addAttribute(Self.colorKey, value: Self.cgColor(run.rgba), range: NSRange(location: run.range.lowerBound, length: run.range.count))
        }
        let line = CTLineCreateWithAttributedString(text)
        self.line = line
        var a: CGFloat = 0
        var d: CGFloat = 0
        width = ceil(CTLineGetTypographicBounds(line, &a, &d, nil))
        ascent = a
        descent = d
        tints = runs.tints.map { run in
            (CTLineGetOffsetForStringIndex(line, run.range.lowerBound, nil), CTLineGetOffsetForStringIndex(line, run.range.upperBound, nil), Self.cgColor(run.rgba))
        }
    }

    /// Draws the line centred in `height`, in a context whose y runs down, as a `Canvas`'s does.
    func draw(in cg: CGContext, height: CGFloat) {
        let top = (height - ascent - descent) / 2
        for t in tints {
            cg.setFillColor(t.color)
            cg.fill(CGRect(x: t.from, y: top, width: t.to - t.from, height: ascent + descent))
        }
        cg.textMatrix = CGAffineTransform(scaleX: 1, y: -1)
        cg.textPosition = CGPoint(x: 0, y: top + ascent)
        CTLineDraw(line, cg)
    }

    private static let fontKey = NSAttributedString.Key(kCTFontAttributeName as String)
    private static let colorKey = NSAttributedString.Key(kCTForegroundColorAttributeName as String)

    /// SF Mono, as `Typography.codeSmall` draws it.
    private static func font(_ size: CGFloat) -> CTFont {
        #if os(iOS)
            UIFont.monospacedSystemFont(ofSize: size, weight: .regular) as CTFont
        #else
            NSFont.monospacedSystemFont(ofSize: size, weight: .regular) as CTFont
        #endif
    }

    static func cgColor(_ rgba: UInt32) -> CGColor {
        let (r, g, b, a) = Token.components(rgba)
        return CGColor(srgbRed: r, green: g, blue: b, alpha: a)
    }

    /// The footnote size this platform starts from, before Dynamic Type scales it.
    static let footnote: CGFloat = {
        #if os(iOS)
            UIFont.preferredFont(forTextStyle: .footnote).pointSize
        #else
            NSFont.preferredFont(forTextStyle: .footnote).pointSize
        #endif
    }()

    /// The caption size, for a diff's hunk headers.
    static let caption: CGFloat = {
        #if os(iOS)
            UIFont.preferredFont(forTextStyle: .caption1).pointSize
        #else
            NSFont.preferredFont(forTextStyle: .caption1).pointSize
        #endif
    }()
}

/// A run of a row: a fill from `x`, `width` wide (or to the row's end), and a text `inset` into it, clipped to it.
struct Cell {
    var x: CGFloat = 0
    var width: CGFloat?
    var fill: CGColor?
    var text: DrawnText?
    var inset: CGFloat = 0
}

/// A block of fixed-height rows drawn in one `Canvas`: a screen of code costs a draw or two rather than a view, a text
/// layout and a layer per row.
///
/// A `Canvas` is rasterized whole, so a block as wide as its longest line would hold a bitmap of lines no one sees. Given
/// its full `width` and the scroll view's `window` over it, a block draws only that window.
@MainActor
struct DrawnRows: View {
    let rows: [[Cell]]
    let rowHeight: CGFloat
    /// The rows' full width; without it they take the width they're offered.
    var width: CGFloat?
    /// The part of `width` on screen: where it starts and how wide it is.
    var window: (x: CGFloat, width: CGFloat)?

    var body: some View {
        let height = CGFloat(rows.count) * rowHeight
        if let width, let window {
            let shown = min(window.width, width)
            let x = min(max(window.x, 0), width - shown)
            canvas(from: x, full: width)
                .frame(width: shown, height: height)
                .offset(x: x)
                .frame(width: width, alignment: .leading)
        } else {
            canvas(from: 0, full: width).frame(width: width, height: height)
        }
    }

    private func canvas(from x0: CGFloat, full: CGFloat?) -> some View {
        Canvas { context, size in
            context.withCGContext { cg in
                cg.translateBy(x: -x0, y: 0)
                let end = full ?? size.width
                for (i, cells) in rows.enumerated() {
                    let y = CGFloat(i) * rowHeight
                    for c in cells {
                        let rect = CGRect(x: c.x, y: y, width: c.width ?? max(end - c.x, 0), height: rowHeight)
                        guard rect.maxX > x0, rect.minX < x0 + size.width else { continue }
                        if let fill = c.fill {
                            cg.setFillColor(fill)
                            cg.fill(rect)
                        }
                        guard let text = c.text else { continue }
                        cg.saveGState()
                        cg.clip(to: rect)
                        cg.translateBy(x: c.x + c.inset, y: y)
                        text.draw(in: cg, height: rowHeight)
                        cg.restoreGState()
                    }
                }
            }
        }
    }

    /// Rows are cut into blocks of this many.
    nonisolated static let block = 32

    /// The rows of block `b` of `count`.
    nonisolated static func range(_ b: Int, count: Int) -> Range<Int> {
        (b * block)..<min((b + 1) * block, count)
    }

    nonisolated static func blocks(_ count: Int) -> Int { (count + block - 1) / block }
}

/// How far a two-way scroll view's content has moved left, read off the content's frame: the macOS 14 SDK has no scroll
/// geometry to ask.
struct ScrollX: PreferenceKey {
    static let defaultValue: CGFloat = 0
    static func reduce(value: inout CGFloat, nextValue: () -> CGFloat) { value = max(value, nextValue()) }
}

extension View {
    /// Reports the content's horizontal offset in the scroll view whose coordinate space is `space`.
    func reportsScrollX(in space: String) -> some View {
        background(GeometryReader { g in Color.clear.preference(key: ScrollX.self, value: max(-g.frame(in: .named(space)).minX, 0)) })
    }
}

/// Rows' drawn texts, made once per row, appearance and size and kept while the rows scroll off and back. A row's
/// plain text is replaced once its colours land.
@MainActor
final class RowTexts<Key: Hashable> {
    private struct Entry {
        let text: DrawnText
        let coloured: Bool
    }

    private var texts: [Key: Entry] = [:]
    /// Many screens either way; past it the cache starts again rather than holding a whole long file.
    private let limit = 4_000

    func text(_ key: Key, coloured: Bool, make: () -> DrawnText) -> DrawnText {
        if let hit = texts[key], hit.coloured || !coloured { return hit.text }
        if texts.count >= limit { texts.removeAll(keepingCapacity: true) }
        let text = make()
        texts[key] = Entry(text: text, coloured: coloured)
        return text
    }
}
