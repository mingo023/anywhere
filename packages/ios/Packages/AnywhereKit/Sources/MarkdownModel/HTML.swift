/// The HTML markdown may hold, read as the desktop's sub-parser reads it (`vendor/gpui-base/src/text/format/html.rs`):
/// blocks, `<br>` as a break, inline marks, links and images. Unsupported tags are dropped and their text kept;
/// script-like tags go with their content. HTML that doesn't tokenize falls back to its source as text.
enum HTML {
    struct Tag: Equatable {
        var name: String
        var attributes: [String: String]
        var closing: Bool
        var selfClosing: Bool
    }

    enum Token: Equatable {
        case text(String)
        case tag(Tag)
    }

    /// Tags dropped with everything inside them (GFM's tag filter list, plus head-only tags).
    static let dropped: Set<String> = ["script", "style", "title", "textarea", "xmp", "iframe", "noembed", "noframes", "plaintext", "head", "noscript", "template"]
    static let void: Set<String> = ["br", "img", "hr", "input", "meta", "link", "wbr", "source", "area", "col", "embed", "param", "track", "base"]
    static let blockTags: Set<String> = [
        "p", "div", "section", "article", "header", "footer", "main", "nav", "aside", "center", "figure", "figcaption",
        "address", "dl", "dt", "dd", "form", "fieldset", "legend", "body", "html", "h1", "h2", "h3", "h4", "h5", "h6",
        "ul", "ol", "li", "blockquote", "pre", "table", "thead", "tbody", "tfoot", "tr", "td", "th", "caption", "hr",
        "details", "summary", "picture",
    ]

    // MARK: Tokens

    /// The tokens of `html`; nil when a tag never closes its `>`.
    static func tokens(_ html: String) -> [Token]? {
        var out: [Token] = []
        var text = ""
        let s = Array(html.unicodeScalars)
        var i = 0
        func flush() {
            if !text.isEmpty { out.append(.text(decode(text))) }
            text = ""
        }
        while i < s.count {
            let c = s[i]
            guard c == "<", i + 1 < s.count, isTagStart(s[i + 1]) else {
                text.unicodeScalars.append(c)
                i += 1
                continue
            }
            if s[i + 1] == "!" || s[i + 1] == "?" {
                // Comments, doctypes, CDATA and processing instructions show nothing.
                let isComment = i + 3 < s.count && s[i + 2] == "-" && s[i + 3] == "-"
                guard let end = isComment ? find(s, "-->", from: i + 4) : find(s, ">", from: i + 2) else { return nil }
                flush()
                i = end
                continue
            }
            guard let (tag, end) = tag(s, at: i) else { return nil }
            flush()
            out.append(.tag(tag))
            i = end
        }
        flush()
        return out
    }

    private static func isTagStart(_ c: Unicode.Scalar) -> Bool {
        c == "/" || c == "!" || c == "?" || (c.isASCII && c.properties.isAlphabetic)
    }

    /// Where `needle` ends, searching from `from`.
    private static func find(_ s: [Unicode.Scalar], _ needle: String, from: Int) -> Int? {
        let n = Array(needle.unicodeScalars)
        var i = from
        while i + n.count <= s.count {
            if s[i] == n[0], Array(s[i..<i + n.count]) == n { return i + n.count }
            i += 1
        }
        return nil
    }

    /// The tag starting at `start` (its `<`) and the index just past its `>`.
    static func tag(_ s: [Unicode.Scalar], at start: Int) -> (Tag, Int)? {
        var i = start + 1
        let closing = s[i] == "/"
        if closing { i += 1 }
        var name = ""
        while i < s.count, s[i].isASCII, s[i].properties.isAlphabetic || ("0"..."9").contains(s[i]) || s[i] == "-" {
            name.unicodeScalars.append(s[i])
            i += 1
        }
        guard !name.isEmpty else { return nil }
        var attributes: [String: String] = [:]
        var selfClosing = false
        while i < s.count {
            let c = s[i]
            if c == ">" { return (Tag(name: name.lowercased(), attributes: attributes, closing: closing, selfClosing: selfClosing), i + 1) }
            if c == "/" {
                selfClosing = true
                i += 1
                continue
            }
            if c.properties.isWhitespace {
                i += 1
                continue
            }
            var key = ""
            while i < s.count, !s[i].properties.isWhitespace, s[i] != "=", s[i] != ">", s[i] != "/" {
                key.unicodeScalars.append(s[i])
                i += 1
            }
            while i < s.count, s[i].properties.isWhitespace { i += 1 }
            var value = ""
            if i < s.count, s[i] == "=" {
                i += 1
                while i < s.count, s[i].properties.isWhitespace { i += 1 }
                if i < s.count, s[i] == "\"" || s[i] == "'" {
                    let quote = s[i]
                    i += 1
                    while i < s.count, s[i] != quote {
                        value.unicodeScalars.append(s[i])
                        i += 1
                    }
                    guard i < s.count else { return nil }
                    i += 1
                } else {
                    while i < s.count, !s[i].properties.isWhitespace, s[i] != ">" {
                        value.unicodeScalars.append(s[i])
                        i += 1
                    }
                }
            }
            if key.isEmpty {
                i += 1
            } else {
                attributes[key.lowercased()] = decode(value)
            }
            selfClosing = false
        }
        return nil
    }

    /// One inline HTML token as cmark hands them over: a lone tag, or nil for a comment or anything else.
    static func inlineTag(_ html: String) -> Tag? {
        let s = Array(html.unicodeScalars)
        guard s.count > 2, s[0] == "<", isTagStart(s[1]), s[1] != "!", s[1] != "?" else { return nil }
        return tag(s, at: 0)?.0
    }

    // MARK: Entities

    static let entities: [String: String] = [
        "amp": "&", "lt": "<", "gt": ">", "quot": "\"", "apos": "'", "nbsp": "\u{a0}", "copy": "©", "reg": "®",
        "trade": "™", "hellip": "…", "mdash": "—", "ndash": "–", "lsquo": "‘", "rsquo": "’", "ldquo": "“", "rdquo": "”",
        "bull": "•", "middot": "·", "times": "×", "divide": "÷", "deg": "°", "larr": "←", "rarr": "→", "uarr": "↑",
        "darr": "↓", "harr": "↔", "check": "✓", "zwj": "\u{200d}", "zwnj": "\u{200c}", "ensp": "\u{2002}", "emsp": "\u{2003}",
        "thinsp": "\u{2009}", "laquo": "«", "raquo": "»", "para": "¶", "sect": "§", "euro": "€", "pound": "£", "yen": "¥", "cent": "¢",
    ]

    static func decode(_ text: String) -> String {
        guard text.contains("&") else { return text }
        var out = ""
        var rest = Substring(text)
        while let amp = rest.firstIndex(of: "&") {
            out += rest[..<amp]
            let after = rest[rest.index(after: amp)...]
            if let semi = after.prefix(12).firstIndex(of: ";") {
                let name = after[..<semi]
                var replacement: String?
                if name.hasPrefix("#x") || name.hasPrefix("#X") {
                    replacement = UInt32(name.dropFirst(2), radix: 16).flatMap(Unicode.Scalar.init).map { String(Character($0)) }
                } else if name.hasPrefix("#") {
                    replacement = UInt32(name.dropFirst()).flatMap(Unicode.Scalar.init).map { String(Character($0)) }
                } else {
                    replacement = entities[String(name)]
                }
                if let replacement {
                    out += replacement
                    rest = after[after.index(after: semi)...]
                    continue
                }
            }
            out += "&"
            rest = after
        }
        return out + rest
    }

    // MARK: Tree

    final class Node {
        let tag: Tag?
        let text: String?
        var children: [Node] = []

        init(tag: Tag?, text: String? = nil) {
            self.tag = tag
            self.text = text
        }

        var name: String { tag?.name ?? "" }
        func attribute(_ key: String) -> String? { tag?.attributes[key] }
    }

    /// The tokens as a tree, closing what's left open and ignoring stray closing tags.
    static func tree(_ tokens: [Token]) -> Node {
        let root = Node(tag: nil)
        var stack = [root]
        var skipping: String?
        for token in tokens {
            switch token {
            case .text(let text):
                if skipping == nil { stack.last!.children.append(Node(tag: nil, text: text)) }
            case .tag(let tag):
                if let skip = skipping {
                    if tag.closing, tag.name == skip { skipping = nil }
                    continue
                }
                if tag.closing {
                    if let at = stack.lastIndex(where: { $0.name == tag.name }), at > 0 { stack.removeSubrange(at...) }
                    continue
                }
                if dropped.contains(tag.name) {
                    if !tag.selfClosing { skipping = tag.name }
                    continue
                }
                // A new paragraph or item closes the open one, as HTML does.
                if ["p", "li", "dt", "dd", "tr", "td", "th"].contains(tag.name), let at = stack.lastIndex(where: { $0.name == tag.name }), at > 0,
                    !stack[at...].contains(where: { ["ul", "ol", "table", "dl"].contains($0.name) && $0 !== stack[at] }) {
                    stack.removeSubrange(at...)
                }
                let node = Node(tag: tag)
                stack.last!.children.append(node)
                if !tag.selfClosing, !void.contains(tag.name) { stack.append(node) }
            }
        }
        return root
    }

    // MARK: Blocks

    /// The blocks an HTML block reads as; nil when it doesn't tokenize.
    static func blocks(_ html: String) -> [MDBlock]? {
        guard let tokens = tokens(html) else { return nil }
        return blocks(of: tree(tokens).children)
    }

    static func blocks(of nodes: [Node]) -> [MDBlock] {
        var out: [MDBlock] = []
        var inline: [MDInline] = []
        func flush() {
            let trimmed = trim(inline)
            if !trimmed.isEmpty { out.append(.paragraph(trimmed)) }
            inline = []
        }
        for node in nodes {
            guard let tag = node.tag, blockTags.contains(tag.name) else {
                inline += inlines(of: [node])
                continue
            }
            flush()
            switch tag.name {
            case "h1", "h2", "h3", "h4", "h5", "h6":
                out.append(.heading(level: Int(String(tag.name.last!))!, trim(inlines(of: node.children))))
            case "ul", "ol":
                let items = node.children.filter { $0.name == "li" }.map { MDListItem(blocks: blocks(of: $0.children)) }
                if !items.isEmpty {
                    out.append(.list(MDList(ordered: tag.name == "ol", start: node.attribute("start").flatMap { Int($0) } ?? 1, items: items)))
                }
            case "li":
                out.append(.list(MDList(ordered: false, items: [MDListItem(blocks: blocks(of: node.children))])))
            case "blockquote":
                out.append(.quote(blocks(of: node.children)))
            case "pre":
                let code = node.children.first(where: { $0.name == "code" })
                let language = code?.attribute("class")?.split(separator: " ").first(where: { $0.hasPrefix("language-") }).map { $0.dropFirst(9).lowercased() }
                var text = rawText(of: node.children)
                if text.hasPrefix("\n") { text.removeFirst() }
                if text.hasSuffix("\n") { text.removeLast() }
                out.append(.code(MDCode(language: language, code: text, fenced: true)))
            case "hr":
                out.append(.rule)
            case "table":
                if let table = table(node) { out.append(.table(table)) }
            case "summary":
                let title = trim(inlines(of: node.children))
                if !title.isEmpty { out.append(.paragraph([.strong(title)])) }
            default:
                out += blocks(of: node.children)
            }
        }
        flush()
        return out
    }

    static func table(_ node: Node) -> MDTable? {
        var rows: [Node] = []
        func collect(_ n: Node) {
            for child in n.children {
                if child.name == "tr" { rows.append(child) } else if ["thead", "tbody", "tfoot"].contains(child.name) { collect(child) }
            }
        }
        collect(node)
        guard let first = rows.first else { return nil }
        let cells = { (row: Node) in row.children.filter { $0.name == "td" || $0.name == "th" } }
        let columns = rows.map { cells($0).count }.max() ?? 0
        guard columns > 0 else { return nil }
        let pad = { (row: [[MDInline]]) in row + Array(repeating: [], count: columns - row.count) }
        let alignments: [MDAlignment] = pad(cells(first).map { _ in [] }).indices.map { i in
            let cell = cells(first)
            guard i < cell.count else { return .none }
            switch cell[i].attribute("align")?.lowercased() {
            case "left": return .left
            case "center": return .center
            case "right": return .right
            default: return .none
            }
        }
        let read = { (row: Node) in pad(cells(row).map { trim(inlines(of: $0.children)) }) }
        return MDTable(alignments: alignments, header: read(first), rows: rows.dropFirst().map(read))
    }

    // MARK: Inlines

    static func inlines(of nodes: [Node]) -> [MDInline] {
        var out: [MDInline] = []
        for node in nodes {
            guard let tag = node.tag else {
                append(&out, .text(collapse(node.text ?? "")))
                continue
            }
            let children = { inlines(of: node.children) }
            switch tag.name {
            case "br": out.append(.hardBreak)
            case "img":
                if let image = image(tag) { out.append(.image(image)) }
            case "b", "strong": out.append(.strong(children()))
            case "i", "em", "cite", "dfn", "var": out.append(.emphasis(children()))
            case "s", "del", "strike": out.append(.strikethrough(children()))
            case "u", "ins": out.append(.underline(children()))
            case "code", "kbd", "samp", "tt": out.append(.code(rawText(of: node.children)))
            case "a":
                if let href = tag.attributes["href"] {
                    out.append(.link(destination: href, title: tag.attributes["title"], children()))
                } else {
                    out += children()
                }
            default:
                if blockTags.contains(tag.name) {
                    // A block inside an inline run reads as a line of its own.
                    out.append(.hardBreak)
                    out += children()
                    out.append(.hardBreak)
                } else {
                    out += children()
                }
            }
        }
        return out
    }

    static func image(_ tag: Tag) -> MDImage? {
        guard let src = tag.attributes["src"] else { return nil }
        let size = { (key: String) in tag.attributes[key].flatMap { Double($0.replacingOccurrences(of: "px", with: "")) } }
        return MDImage(source: src, alt: tag.attributes["alt"] ?? "", title: tag.attributes["title"], width: size("width"), height: size("height"))
    }

    static func rawText(of nodes: [Node]) -> String {
        nodes.map { $0.text ?? ($0.name == "br" ? "\n" : rawText(of: $0.children)) }.joined()
    }

    static func collapse(_ text: String) -> String {
        var out = ""
        var space = false
        for c in text {
            if c.isWhitespace, c != "\u{a0}" {
                space = true
            } else {
                if space { out.append(" ") }
                space = false
                out.append(c)
            }
        }
        if space { out.append(" ") }
        return out
    }

    static func append(_ out: inout [MDInline], _ inline: MDInline) {
        if case .text(let new) = inline, case .text(let old)? = out.last {
            out[out.count - 1] = .text(old + new)
        } else {
            out.append(inline)
        }
    }

    /// The run without leading or trailing spaces or breaks, and without a space hugging a break.
    static func trim(_ inlines: [MDInline]) -> [MDInline] {
        var out: [MDInline] = []
        for inline in inlines {
            switch inline {
            case .text(var s):
                if out.isEmpty || out.last == .hardBreak { s = String(s.drop(while: { $0 == " " })) }
                if !s.isEmpty { append(&out, .text(s)) }
            case .hardBreak:
                if case .text(let s)? = out.last {
                    let t = String(s.reversed().drop(while: { $0 == " " }).reversed())
                    out.removeLast()
                    if !t.isEmpty { out.append(.text(t)) }
                }
                if !out.isEmpty { out.append(.hardBreak) }
            default:
                out.append(inline)
            }
        }
        while out.last == .hardBreak { out.removeLast() }
        if case .text(let s)? = out.last {
            let t = String(s.reversed().drop(while: { $0 == " " }).reversed())
            out.removeLast()
            if !t.isEmpty { out.append(.text(t)) }
        }
        return out
    }
}
