extension Lexer {
    // MARK: HTML

    mutating func html() {
        while i < end {
            let c = b[i]
            let start = i
            guard c == UInt8(ascii: "<") else {
                i += 1
                continue
            }
            if matches("<!--", at: i) {
                i += 4
                while i < end, !matches("-->", at: i) { i += 1 }
                i = min(end, i + 3)
                comment(start, i)
            } else if at(i + 1) == UInt8(ascii: "!") || at(i + 1) == UInt8(ascii: "?") {
                while i < end, b[i] != UInt8(ascii: ">") { i += 1 }
                i = min(end, i + 1)
                emit(.keyword, start, i)
            } else if isLetter(at(i + 1)) || (at(i + 1) == UInt8(ascii: "/") && isLetter(at(i + 2))) {
                let closing = at(i + 1) == UInt8(ascii: "/")
                i += closing ? 2 : 1
                let name = i
                while i < end, isIdent(b[i]) || b[i] == UInt8(ascii: "-") || b[i] == UInt8(ascii: ":") { i += 1 }
                emit(.keyword, name, i)
                let tag = String(decoding: UnsafeBufferPointer(rebasing: b[name..<i]), as: UTF8.self).lowercased()
                var scriptType: String?
                tagAttributes(scriptType: &scriptType)
                if !closing, tag == "script" || tag == "style" {
                    embedded(tag == "style" ? .css : scriptType.map(Self.scriptLanguage) ?? .javascript, until: tag)
                }
            } else {
                i += 1
            }
        }
    }

    static func scriptLanguage(_ type: String) -> Language {
        let t = type.lowercased()
        if t.contains("json") { return .json }
        if t.contains("typescript") { return .typescript }
        if t.isEmpty || t.contains("javascript") || t == "module" { return .javascript }
        return .text
    }

    /// A tag's attributes through its `>`: names as attributes, values as strings.
    mutating func tagAttributes(scriptType: inout String?) {
        while i < end {
            let c = b[i]
            if c == UInt8(ascii: ">") {
                i += 1
                return
            }
            if isSpace(c) || c == UInt8(ascii: "/") {
                i += 1
                continue
            }
            if c == UInt8(ascii: "=") {
                i += 1
                while i < end, isSpace(b[i]) { i += 1 }
                let start = i
                if at(i) == UInt8(ascii: "\"") || at(i) == UInt8(ascii: "'") {
                    let q = b[i]
                    i += 1
                    while i < end, b[i] != q { i += 1 }
                    i = min(end, i + 1)
                } else {
                    while i < end, !isSpace(b[i]), b[i] != UInt8(ascii: ">") { i += 1 }
                }
                emit(.string, start, i)
                if scriptType == "" {
                    let inner = at(start) == UInt8(ascii: "\"") || at(start) == UInt8(ascii: "'") ? (start + 1, max(start + 1, i - 1)) : (start, i)
                    scriptType = String(decoding: UnsafeBufferPointer(rebasing: b[inner.0..<inner.1]), as: UTF8.self)
                }
                continue
            }
            let start = i
            while i < end, !isSpace(b[i]), b[i] != UInt8(ascii: "="), b[i] != UInt8(ascii: ">"), b[i] != UInt8(ascii: "/") { i += 1 }
            if i == start { i += 1 }
            emit(.attribute, start, i)
            if matches("type", at: start), i - start == 4, scriptType == nil { scriptType = "" }
        }
    }

    /// The content of a `<script>` or `<style>` up to its closing tag, in its own language.
    mutating func embedded(_ language: Language, until tag: String) {
        let start = i
        var k = i
        let close = Array("</\(tag)".utf8)
        search: while k < end {
            if b[k] == UInt8(ascii: "<"), k + close.count <= end {
                var same = true
                for j in 0..<close.count where b[k + j] | (j > 1 ? 0x20 : 0) != close[j] { same = false; break }
                if same { break search }
            }
            k += 1
        }
        run(language, start, k)
        i = k
    }

    /// Lexes `start..<stop` as `language`, its spans after those found so far.
    mutating func run(_ language: Language, _ start: Int, _ stop: Int) {
        guard start < stop else { return }
        var inner = Lexer(b, start: start, end: stop)
        inner.lex(language)
        for span in inner.out { emit(span.kind, span.start, span.end) }
    }

    // MARK: Markdown

    mutating func markdown() {
        var fence: (ch: UInt8, len: Int, body: Int, language: Language)?
        while i < end {
            let lineStart = i
            var k = i
            while k < end, b[k] == 0x20 { k += 1 }
            let indent = k - lineStart
            var lineEnd = k
            while lineEnd < end, b[lineEnd] != 0x0A { lineEnd += 1 }
            if let open = fence {
                var run = 0
                while k + run < lineEnd, b[k + run] == open.ch { run += 1 }
                var rest = k + run
                while rest < lineEnd, isSpace(b[rest]) { rest += 1 }
                if indent <= 3, run >= open.len, rest == lineEnd {
                    if open.language == .text { emit(.string, open.body, lineStart) } else { self.run(open.language, open.body, lineStart) }
                    emit(.string, lineStart, lineEnd)
                    fence = nil
                }
                i = lineEnd < end ? lineEnd + 1 : end
                continue
            }
            let c = at(k)
            if indent <= 3, c == UInt8(ascii: "`") || c == UInt8(ascii: "~") {
                var run = 0
                while k + run < lineEnd, b[k + run] == c { run += 1 }
                let infoHasBacktick = c == UInt8(ascii: "`") && (k + run..<lineEnd).contains { b[$0] == c }
                if run >= 3, !infoHasBacktick {
                    var w = k + run
                    while w < lineEnd, b[w] == 0x20 { w += 1 }
                    let word = w
                    while w < lineEnd, !isSpace(b[w]) { w += 1 }
                    let label = String(decoding: UnsafeBufferPointer(rebasing: b[word..<w]), as: UTF8.self)
                    emit(.string, lineStart, lineEnd)
                    fence = (c, run, lineEnd < end ? lineEnd + 1 : end, Language(id: Highlighter.language(for: label)))
                    i = fence!.body
                    continue
                }
            }
            if indent <= 3, c == UInt8(ascii: "#") {
                var hashes = 0
                while k + hashes < lineEnd, b[k + hashes] == UInt8(ascii: "#") { hashes += 1 }
                if hashes <= 6, k + hashes == lineEnd || b[k + hashes] == 0x20 || b[k + hashes] == 0x09 {
                    emit(.title, k, lineEnd)
                    i = lineEnd < end ? lineEnd + 1 : end
                    continue
                }
            }
            i = k
            // Quote and list markers lead the line.
            while i < lineEnd {
                let m = b[i]
                if m == UInt8(ascii: ">") {
                    emit(.keyword, i, i + 1)
                    i += 1
                } else if (m == UInt8(ascii: "-") || m == UInt8(ascii: "*") || m == UInt8(ascii: "+")), at(i + 1) == 0x20 || i + 1 == lineEnd {
                    emit(.keyword, i, i + 1)
                    i += 1
                    if matches(" [ ]", at: i) || matches(" [x]", at: i) || matches(" [X]", at: i) {
                        emit(.keyword, i + 1, i + 4)
                        i += 4
                    }
                } else if isDigit(m) {
                    var d = i
                    while d < lineEnd, isDigit(b[d]) { d += 1 }
                    guard d - i <= 9, at(d) == UInt8(ascii: ".") || at(d) == UInt8(ascii: ")"), at(d + 1) == 0x20 || d + 1 == lineEnd else { break }
                    emit(.keyword, i, d + 1)
                    i = d + 1
                } else if m == 0x20 || m == 0x09 {
                    i += 1
                } else {
                    break
                }
            }
            markdownInline(lineEnd)
            i = lineEnd < end ? lineEnd + 1 : end
        }
        if let open = fence {
            if open.language == .text { emit(.string, open.body, end) } else { run(open.language, open.body, end) }
        }
    }

    /// Code spans, links' destinations, autolinks, web addresses and HTML in one line.
    mutating func markdownInline(_ lineEnd: Int) {
        while i < lineEnd {
            let c = b[i]
            let start = i
            if c == UInt8(ascii: "\\") {
                i += 2
            } else if c == UInt8(ascii: "`") {
                var run = 0
                while at(i + run) == c { run += 1 }
                var k = i + run
                var closed = false
                while k < lineEnd {
                    if b[k] == c {
                        var r = 0
                        while at(k + r) == c { r += 1 }
                        if r == run {
                            k += r
                            closed = true
                            break
                        }
                        k += r
                    } else {
                        k += 1
                    }
                }
                i = closed ? k : i + run
                if closed { emit(.string, start, i) }
            } else if c == UInt8(ascii: "]"), at(i + 1) == UInt8(ascii: "(") {
                i += 2
                let url = i
                while i < lineEnd, b[i] != UInt8(ascii: ")"), b[i] != 0x20 { i += 1 }
                emit(.url, url, i)
            } else if c == UInt8(ascii: "<"), matchesURL(at: i + 1) {
                i += 1
                let url = i
                while i < lineEnd, b[i] != UInt8(ascii: ">") { i += 1 }
                emit(.url, url, i)
            } else if c == UInt8(ascii: "h"), matchesURL(at: i), i == 0 || !isIdent(b[i - 1]) {
                while i < lineEnd, !isURLEnd(b[i]) { i += 1 }
                while i > start, ".,;:!?".utf8.contains(b[i - 1]) { i -= 1 }
                emit(.url, start, i)
            } else if c == UInt8(ascii: "<"), matches("<!--", at: i) {
                while i < lineEnd, !matches("-->", at: i) { i += 1 }
                i = min(lineEnd, i + 3)
                comment(start, i)
            } else if c == UInt8(ascii: "<"), isLetter(at(i + 1)) || (at(i + 1) == UInt8(ascii: "/") && isLetter(at(i + 2))) {
                i += at(i + 1) == UInt8(ascii: "/") ? 2 : 1
                let name = i
                while i < lineEnd, isIdent(b[i]) || b[i] == UInt8(ascii: "-") { i += 1 }
                emit(.keyword, name, i)
            } else {
                i += 1
            }
        }
    }
}
