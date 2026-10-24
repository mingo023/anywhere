import cmark_gfm
import cmark_gfm_extensions

typealias CNode = UnsafeMutablePointer<cmark_node>

/// cmark-gfm with GitHub's extensions, read into `MDBlock`s the way the desktop reads markdown-rs's tree
/// (`vendor/gpui-base/src/text/format/markdown.rs ast_to_node`).
enum Parser {
    /// HTML on so HTML nodes reach us; smart punctuation off; footnotes on; source lines for the streaming split.
    static let options = CMARK_OPT_UNSAFE | CMARK_OPT_FOOTNOTES | CMARK_OPT_SOURCEPOS

    nonisolated(unsafe) static let extensions: [UnsafeMutablePointer<cmark_syntax_extension>] = {
        cmark_gfm_core_extensions_ensure_registered()
        let names = ["table", "strikethrough", "tasklist", "autolink", "tagfilter"]
        return names.compactMap { name in name.withCString { cmark_find_syntax_extension($0) } }
    }()

    /// Each top-level node's blocks with the 0-based line it starts on, then the footnotes section if any.
    static func parse(_ text: Substring) -> (top: [(blocks: [MDBlock], line: Int)], footnotes: MDBlock?) {
        var (source, originals) = Math.rewrite(String(text))
        guard let parser = cmark_parser_new(options) else { return ([], nil) }
        defer { cmark_parser_free(parser) }
        for ext in extensions { cmark_parser_attach_syntax_extension(parser, ext) }
        source.withUTF8 { buffer in
            buffer.withMemoryRebound(to: CChar.self) { cmark_parser_feed(parser, $0.baseAddress, $0.count) }
        }
        guard let root = cmark_parser_finish(parser) else { return ([], nil) }
        defer { cmark_node_free(root) }
        cmark_consolidate_text_nodes(root)

        var reader = Reader(originals: originals)
        var top: [(blocks: [MDBlock], line: Int)] = []
        var definitions: [CNode] = []
        var node = cmark_node_first_child(root)
        while let n = node {
            if cmark_node_get_type(n) == CMARK_NODE_FOOTNOTE_DEFINITION {
                definitions.append(n)
            } else {
                top.append((reader.blocks(n), Int(cmark_node_get_start_line(n)) - 1))
            }
            node = cmark_node_next(n)
        }
        // cmark-gfm leaves the paragraph a table was cut from without a start line, and dates the table to the
        // paragraph's first line.
        for k in top.indices.reversed() where top[k].line < 0 { top[k].line = k + 1 < top.count ? top[k + 1].line : 0 }
        // cmark-gfm has numbered the references, dropped unreferenced definitions and moved the rest to the end, in order.
        let bodies = definitions.map { reader.children(of: $0) }
        let notes = bodies.enumerated().map { MDFootnote(number: $0.offset + 1, blocks: $0.element, references: reader.references[$0.offset + 1, default: 0]) }
        return (top, notes.isEmpty ? nil : .footnotes(notes))
    }
}

/// Walks cmark's tree into blocks and inlines.
struct Reader {
    /// References to each footnote number seen so far.
    var references: [Int: Int] = [:]
    var originals: Math.Originals = [:]

    mutating func children(of node: CNode) -> [MDBlock] {
        var out: [MDBlock] = []
        var child = cmark_node_first_child(node)
        while let c = child {
            out += blocks(c)
            child = cmark_node_next(c)
        }
        return out
    }

    mutating func blocks(_ node: CNode) -> [MDBlock] {
        switch cmark_node_get_type(node) {
        case CMARK_NODE_PARAGRAPH:
            let content = inlines(of: node)
            return content.isEmpty ? [] : [.paragraph(content)]
        case CMARK_NODE_HEADING:
            return [.heading(level: Int(cmark_node_get_heading_level(node)), inlines(of: node))]
        case CMARK_NODE_BLOCK_QUOTE:
            return [quote(node)]
        case CMARK_NODE_LIST:
            var items: [MDListItem] = []
            var child = cmark_node_first_child(node)
            while let item = child {
                let checked = typeString(item) == "tasklist" ? cmark_gfm_extensions_get_tasklist_item_checked(item) : nil
                items.append(MDListItem(checked: checked, blocks: children(of: item)))
                child = cmark_node_next(item)
            }
            let ordered = cmark_node_get_list_type(node) == CMARK_ORDERED_LIST
            return [.list(MDList(ordered: ordered, start: ordered ? Int(cmark_node_get_list_start(node)) : 1, tight: cmark_node_get_list_tight(node) != 0, items: items))]
        case CMARK_NODE_CODE_BLOCK:
            var code = string(cmark_node_get_literal(node))
            if code.hasSuffix("\n") { code.removeLast() }
            let info = string(cmark_node_get_fence_info(node))
            let word = info.split(whereSeparator: \.isWhitespace).first.map { $0.lowercased() }
            if word == Math.info.lowercased() { return [.code(MDCode(language: nil, code: code, fenced: false))] }
            var length: Int32 = 0, offset: Int32 = 0
            var character: CChar = 0
            let fenced = cmark_node_get_fenced(node, &length, &offset, &character) != 0
            code = Math.restore(code, from: Int(cmark_node_get_start_line(node)) - (fenced ? 0 : 1), originals)
            return [.code(MDCode(language: word, code: code, fenced: fenced))]
        case CMARK_NODE_HTML_BLOCK:
            var html = string(cmark_node_get_literal(node))
            if html.hasSuffix("\n") { html.removeLast() }
            html = Math.restore(html, from: Int(cmark_node_get_start_line(node)) - 1, originals)
            return HTML.blocks(html) ?? [.paragraph([.text(html)])]
        case CMARK_NODE_THEMATIC_BREAK:
            return [.rule]
        case CMARK_NODE_FOOTNOTE_DEFINITION:
            return []
        default:
            return typeString(node) == "table" ? [table(node)] : []
        }
    }

    /// A quote, or a GitHub alert when its first paragraph opens with an alert tag.
    /// The title is the rest of the tag's line, else the kind's name; the paragraph's later lines open the body.
    mutating func quote(_ node: CNode) -> MDBlock {
        var body = children(of: node)
        guard case .paragraph(var first)? = body.first, case .text(let opening)? = first.first, let (kind, length) = MDAlertKind.parse(opening) else {
            return .quote(body)
        }
        let rest = opening.dropFirst(length)
        first[0] = .text(String(rest.hasPrefix(" ") ? rest.dropFirst() : rest))
        let lineEnd = first.firstIndex(where: { $0 == .softBreak || $0 == .hardBreak })
        var title = Array(first[..<(lineEnd ?? first.endIndex)])
        let after = lineEnd.map { Array(first[($0 + 1)...]) } ?? []
        if after.allSatisfy(Self.isBlank) {
            body.removeFirst()
        } else {
            body[0] = .paragraph(after)
        }
        if title.allSatisfy(Self.isBlank) { title = [.text(kind.title)] }
        return .alert(MDAlert(kind: kind, title: title, body: body))
    }

    static func isBlank(_ inline: MDInline) -> Bool {
        if case .text(let s) = inline { return s.allSatisfy(\.isWhitespace) }
        return false
    }

    mutating func table(_ node: CNode) -> MDBlock {
        let columns = Int(cmark_gfm_extensions_get_table_columns(node))
        let aligns = cmark_gfm_extensions_get_table_alignments(node)
        let alignments: [MDAlignment] = (0..<columns).map { i in
            switch aligns?[i] {
            case UInt8(ascii: "l"): .left
            case UInt8(ascii: "c"): .center
            case UInt8(ascii: "r"): .right
            default: .none
            }
        }
        var header: [[MDInline]] = []
        var rows: [[[MDInline]]] = []
        var row = cmark_node_first_child(node)
        while let r = row {
            var cells: [[MDInline]] = []
            var cell = cmark_node_first_child(r)
            while let c = cell {
                cells.append(inlines(of: c))
                cell = cmark_node_next(c)
            }
            cells += Array(repeating: [], count: max(0, columns - cells.count))
            if typeString(r) == "table_header" { header = cells } else { rows.append(cells) }
            row = cmark_node_next(r)
        }
        return .table(MDTable(alignments: alignments, header: header, rows: rows))
    }

    // MARK: Inlines

    /// An inline HTML tag left open, gathering what follows it until its closing tag.
    struct Open {
        let tag: HTML.Tag
        var content: [MDInline] = []
    }

    mutating func inlines(of node: CNode) -> [MDInline] {
        var out: [MDInline] = []
        var open: [Open] = []
        func push(_ inline: MDInline) {
            if open.isEmpty { HTML.append(&out, inline) } else { HTML.append(&open[open.count - 1].content, inline) }
        }
        var child = cmark_node_first_child(node)
        while let c = child {
            child = cmark_node_next(c)
            guard cmark_node_get_type(c) == CMARK_NODE_HTML_INLINE else {
                inline(c).map(push)
                continue
            }
            guard let tag = HTML.inlineTag(string(cmark_node_get_literal(c))) else { continue }
            if tag.closing {
                guard let at = open.lastIndex(where: { $0.tag.name == tag.name }) else { continue }
                // Tags opened inside it and never closed don't apply.
                while open.count > at + 1 {
                    let unclosed = open.removeLast()
                    unclosed.content.forEach { HTML.append(&open[open.count - 1].content, $0) }
                }
                let done = open.removeLast()
                if let wrapped = Self.wrap(done.tag, done.content) { push(wrapped) } else { done.content.forEach(push) }
                continue
            }
            switch tag.name {
            case "br": push(.hardBreak)
            case "img": HTML.image(tag).map { push(.image($0)) }
            default:
                if !tag.selfClosing, !HTML.void.contains(tag.name) { open.append(Open(tag: tag)) }
            }
        }
        while let unclosed = open.popLast() { unclosed.content.forEach(push) }
        return out
    }

    static func wrap(_ tag: HTML.Tag, _ content: [MDInline]) -> MDInline? {
        switch tag.name {
        case "b", "strong": .strong(content)
        case "i", "em", "cite", "dfn", "var": .emphasis(content)
        case "s", "del", "strike": .strikethrough(content)
        case "u", "ins": .underline(content)
        case "code", "kbd", "samp", "tt": .code(content.text)
        case "a": tag.attributes["href"].map { .link(destination: $0, title: tag.attributes["title"], content) }
        default: nil
        }
    }

    mutating func inline(_ node: CNode) -> MDInline? {
        switch cmark_node_get_type(node) {
        case CMARK_NODE_TEXT:
            let text = string(cmark_node_get_literal(node))
            return text.isEmpty ? nil : .text(text)
        case CMARK_NODE_SOFTBREAK:
            return .softBreak
        case CMARK_NODE_LINEBREAK:
            return .hardBreak
        case CMARK_NODE_CODE:
            let code = string(cmark_node_get_literal(node))
            return pathFolder(in: code).map { .path(code, folder: $0) } ?? .code(code)
        case CMARK_NODE_EMPH:
            return .emphasis(inlines(of: node))
        case CMARK_NODE_STRONG:
            return .strong(inlines(of: node))
        case CMARK_NODE_LINK:
            return .link(destination: string(cmark_node_get_url(node)), title: optional(cmark_node_get_title(node)), inlines(of: node))
        case CMARK_NODE_IMAGE:
            return .image(MDImage(source: string(cmark_node_get_url(node)), alt: inlines(of: node).text, title: optional(cmark_node_get_title(node))))
        case CMARK_NODE_FOOTNOTE_REFERENCE:
            guard let number = Int(string(cmark_node_get_literal(node))) else { return nil }
            references[number, default: 0] += 1
            return .footnote(number)
        default:
            return typeString(node) == "strikethrough" ? .strikethrough(inlines(of: node)) : nil
        }
    }

    func typeString(_ node: CNode) -> String {
        guard node.pointee.extension != nil else { return "" }
        return string(cmark_node_get_type_string(node))
    }

    func string(_ p: UnsafePointer<CChar>?) -> String {
        p.map { String(cString: $0) } ?? ""
    }

    func optional(_ p: UnsafePointer<CChar>?) -> String? {
        let s = string(p)
        return s.isEmpty ? nil : s
    }
}
