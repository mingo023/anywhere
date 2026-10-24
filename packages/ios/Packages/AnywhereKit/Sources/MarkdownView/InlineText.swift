import DesignSystem
import MarkdownModel
import SwiftUI

/// How a run of inlines is drawn: its size, and what its block does to it (a heading's weight, a ticked task's muting).
struct InlineStyle: Equatable {
    var size: CGFloat
    var weight: Font.Weight = .regular
    var color: Token = Palette.textBody
    var muted = false
}

/// Links carry their destination as written inside this scheme, so a tap hands back exactly what the markdown said.
enum LinkURL {
    static let scheme = "anywhere-md"

    nonisolated static func url(for destination: String) -> URL? {
        var parts = URLComponents()
        parts.scheme = scheme
        parts.path = "link"
        parts.queryItems = [URLQueryItem(name: "to", value: destination)]
        return parts.url
    }

    nonisolated static func destination(of url: URL) -> String? {
        guard url.scheme == scheme else { return nil }
        return URLComponents(url: url, resolvingAgainstBaseURL: false)?.queryItems?.first { $0.name == "to" }?.value
    }
}

/// A paragraph's inlines as one `Text`, so it lays out, wraps and selects as one run.
struct InlineText {
    let style: InlineStyle

    private struct Marks {
        var bold = false
        var italic = false
        var strike = false
        var underline = false
        var link: String?
    }

    private var pieces: [Text] = []
    private var run = AttributedString()

    init(style: InlineStyle) { self.style = style }

    /// The inlines as text; images are left to the caller.
    static func text(_ inlines: [MDInline], style: InlineStyle) -> Text {
        var builder = InlineText(style: style)
        builder.add(inlines, Marks())
        return builder.finish()
    }

    private mutating func finish() -> Text {
        flush()
        return pieces.dropFirst().reduce(pieces.first ?? Text("")) { $0 + $1 }
    }

    private mutating func flush() {
        guard !run.characters.isEmpty else { return }
        pieces.append(Text(run))
        run = AttributedString()
    }

    private func font(_ marks: Marks, mono: Bool = false) -> Font {
        let size = mono ? style.size * 0.9 : style.size
        var font = Font.system(size: size, weight: marks.bold ? .semibold : style.weight, design: mono ? .monospaced : .default)
        if marks.italic { font = font.italic() }
        return font
    }

    private mutating func append(_ string: String, _ marks: Marks, mono: Bool = false, color: Token? = nil, background: Token? = nil) {
        var piece = AttributedString(string)
        piece.font = font(marks, mono: mono)
        let strong = marks.bold || marks.link != nil || mono
        piece.foregroundColor = (style.muted ? Palette.text3 : color ?? (strong ? Palette.text : style.color)).color
        if let background { piece.backgroundColor = background.color }
        if marks.strike || style.muted { piece.strikethroughStyle = Text.LineStyle(pattern: .solid, color: nil) }
        if let link = marks.link {
            piece.link = LinkURL.url(for: link)
            piece.underlineStyle = Text.LineStyle(pattern: .solid, color: Palette.text2.color)
        } else if marks.underline {
            piece.underlineStyle = Text.LineStyle(pattern: .solid, color: nil)
        }
        run.append(piece)
    }

    private mutating func add(_ inlines: [MDInline], _ marks: Marks) {
        for inline in inlines {
            switch inline {
            case .text(let s): append(s, marks)
            case .softBreak: append(" ", marks)
            case .hardBreak: append("\n", marks)
            case .code(let code):
                // Thin spaces inside the fill stand in for the desktop's 0.25em × 0.5em padding.
                append("\u{2009}\(code)\u{2009}", marks, mono: true, background: Palette.fill4)
            case .path(let path, let folder):
                var chip = marks
                if !folder { chip.link = path }
                flush()
                let icon = folder ? "folder" : "doc.text"
                let tint = LanguageBadge(path: path).letters.isEmpty ? Palette.text2.color : LanguageBadge(path: path).color
                pieces.append(Text(Image(systemName: icon)).font(.system(size: style.size * 0.8)).foregroundColor(tint))
                var piece = AttributedString("\u{2009}\(path)\u{2009}")
                piece.font = font(Marks(), mono: true)
                piece.foregroundColor = Palette.text.color
                piece.backgroundColor = Palette.fill4.color
                if let link = chip.link { piece.link = LinkURL.url(for: link) }
                run.append(piece)
            case .emphasis(let c):
                var m = marks
                m.italic = true
                add(c, m)
            case .strong(let c):
                var m = marks
                m.bold = true
                add(c, m)
            case .strikethrough(let c):
                var m = marks
                m.strike = true
                add(c, m)
            case .underline(let c):
                var m = marks
                m.underline = true
                add(c, m)
            case .link(let destination, _, let c):
                var m = marks
                m.link = destination
                add(c, m)
            case .image(let image):
                // Images inside a text run (a link around one, say) read as their alt text.
                append(image.alt.isEmpty ? "🖼" : image.alt, marks, color: Palette.text2)
            case .footnote(let n):
                append(superscript(n), marks, color: Palette.text2)
            }
        }
    }
}
