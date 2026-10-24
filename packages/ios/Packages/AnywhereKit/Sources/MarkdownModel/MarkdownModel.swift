/// A run of inline content, as the desktop's markdown renderer reads it (`vendor/gpui-base/src/text/format/markdown.rs`).
public indirect enum MDInline: Sendable, Hashable {
    case text(String)
    /// A bare line ending inside a paragraph; it reads as a space.
    case softBreak
    /// Two trailing spaces, a trailing backslash or `<br>`; it keeps its newline.
    case hardBreak
    case code(String)
    /// Inline code that names a file or folder (`path(in:)`), drawn as a chip.
    case path(String, folder: Bool)
    case emphasis([MDInline])
    case strong([MDInline])
    case strikethrough([MDInline])
    /// From inline `<u>`.
    case underline([MDInline])
    /// A link with its destination as written (reference links resolved).
    case link(destination: String, title: String?, [MDInline])
    case image(MDImage)
    /// A footnote reference, numbered in reference order.
    case footnote(Int)
}

public struct MDImage: Sendable, Hashable {
    public var source: String
    public var alt: String
    public var title: String?
    public var width: Double?
    public var height: Double?

    public init(source: String, alt: String, title: String? = nil, width: Double? = nil, height: Double? = nil) {
        self.source = source
        self.alt = alt
        self.title = title
        self.width = width
        self.height = height
    }
}

/// A block of a markdown document.
public indirect enum MDBlock: Sendable, Hashable {
    case paragraph([MDInline])
    case heading(level: Int, [MDInline])
    case list(MDList)
    case quote([MDBlock])
    /// A GitHub alert: a quote opening with `[!NOTE]` and the like.
    case alert(MDAlert)
    case code(MDCode)
    case table(MDTable)
    case rule
    /// The footnotes section the document ends with.
    case footnotes([MDFootnote])
}

public struct MDList: Sendable, Hashable {
    public var ordered: Bool
    /// The first item's number; an ordered list counts from it.
    public var start: Int
    public var tight: Bool
    public var items: [MDListItem]

    public init(ordered: Bool, start: Int = 1, tight: Bool = true, items: [MDListItem]) {
        self.ordered = ordered
        self.start = start
        self.tight = tight
        self.items = items
    }
}

public struct MDListItem: Sendable, Hashable {
    /// nil for a plain item, else whether its task box is ticked.
    public var checked: Bool?
    public var blocks: [MDBlock]

    public init(checked: Bool? = nil, blocks: [MDBlock]) {
        self.checked = checked
        self.blocks = blocks
    }
}

public enum MDAlertKind: String, Sendable, Hashable, CaseIterable {
    case note, tip, important, warning, caution

    /// The title an alert shows when its tag line has none.
    public var title: String { rawValue.prefix(1).uppercased() + rawValue.dropFirst() }

    /// The kind's colour, 0xRRGGBB, as GitHub draws it.
    public var rgb: UInt32 {
        switch self {
        case .note: 0x0969da
        case .tip: 0x1a7f37
        case .important: 0x8250df
        case .warning: 0x9a6700
        case .caution: 0xd1242f
        }
    }

    /// The kind whose tag `text` opens with, in any case, and the tag's length in characters.
    static func parse(_ text: String) -> (MDAlertKind, Int)? {
        for kind in allCases {
            let tag = "[!\(kind.rawValue)]"
            if text.count >= tag.count, text.prefix(tag.count).lowercased() == tag { return (kind, tag.count) }
        }
        return nil
    }
}

public struct MDAlert: Sendable, Hashable {
    public var kind: MDAlertKind
    public var title: [MDInline]
    public var body: [MDBlock]

    public init(kind: MDAlertKind, title: [MDInline], body: [MDBlock]) {
        self.kind = kind
        self.title = title
        self.body = body
    }
}

public struct MDCode: Sendable, Hashable {
    /// The info string's first word, lowercased; nil when it has none.
    public var language: String?
    public var code: String
    /// Written between fences rather than indented (or as `$$` math, which counts as unfenced).
    public var fenced: Bool

    public init(language: String?, code: String, fenced: Bool) {
        self.language = language
        self.code = code
        self.fenced = fenced
    }

    /// What the block's header reads: its language, else "code" for a fence or "text" for indented code.
    public var label: String { language ?? (fenced ? "code" : "text") }
}

public enum MDAlignment: Sendable, Hashable {
    case none, left, center, right
}

public struct MDTable: Sendable, Hashable {
    public var alignments: [MDAlignment]
    public var header: [[MDInline]]
    public var rows: [[[MDInline]]]

    public init(alignments: [MDAlignment], header: [[MDInline]], rows: [[[MDInline]]]) {
        self.alignments = alignments
        self.header = header
        self.rows = rows
    }
}

public struct MDFootnote: Sendable, Hashable {
    public var number: Int
    public var blocks: [MDBlock]
    /// How many times the text refers to it; the section shows one "↩" each.
    public var references: Int

    public init(number: Int, blocks: [MDBlock], references: Int) {
        self.number = number
        self.blocks = blocks
        self.references = references
    }
}

// MARK: - Plain text

extension MDInline {
    /// What the run reads as: soft breaks as spaces, hard breaks as newlines, footnotes as superscript numbers.
    public var text: String {
        switch self {
        case .text(let s), .code(let s): s
        case .path(let s, _): s
        case .softBreak: " "
        case .hardBreak: "\n"
        case .emphasis(let c), .strong(let c), .strikethrough(let c), .underline(let c), .link(_, _, let c): c.text
        case .image(let image): image.alt
        case .footnote(let n): superscript(n)
        }
    }
}

extension [MDInline] {
    public var text: String { map(\.text).joined() }
}

extension MDBlock {
    /// The block's text, blocks separated by newlines, for tests and accessibility.
    public var text: String {
        switch self {
        case .paragraph(let i), .heading(_, let i): i.text
        case .list(let list): list.items.map { $0.blocks.text }.joined(separator: "\n")
        case .quote(let b): b.text
        case .alert(let a): ([a.title.text] + a.body.map(\.text)).joined(separator: "\n")
        case .code(let c): c.code
        case .table(let t): ([t.header] + t.rows).map { $0.map(\.text).joined(separator: " | ") }.joined(separator: "\n")
        case .rule: ""
        case .footnotes(let notes): notes.map { $0.blocks.text + String(repeating: " ↩", count: $0.references) }.joined(separator: "\n")
        }
    }
}

extension [MDBlock] {
    public var text: String { map(\.text).joined(separator: "\n") }
}

/// A number in superscript digits, as a footnote reference reads.
public func superscript(_ n: Int) -> String {
    let digits: [Character] = ["⁰", "¹", "²", "³", "⁴", "⁵", "⁶", "⁷", "⁸", "⁹"]
    return String(String(n).compactMap { $0.wholeNumberValue.map { digits[$0] } })
}
