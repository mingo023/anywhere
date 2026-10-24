private let yamlConstants = Words("true false yes no on off null True False Yes No On Off Null TRUE FALSE YES NO ON OFF NULL ~")

extension Lexer {
    // MARK: JSON

    mutating func json() {
        while i < end {
            let c = b[i]
            let start = i
            if c == UInt8(ascii: "\"") {
                i += 1
                _ = scanString(quote: c, escapes: true, newlineEnds: true)
                var k = i
                while k < end, isSpace(b[k]) { k += 1 }
                emit(at(k) == UInt8(ascii: ":") ? .property : .string, start, i)
            } else if c == UInt8(ascii: "/"), at(i + 1) == UInt8(ascii: "/") {
                skipLine()
                comment(start, i)
            } else if c == UInt8(ascii: "/"), at(i + 1) == UInt8(ascii: "*") {
                blockComment(nested: false)
                comment(start, i)
            } else if isDigit(c) || (c == UInt8(ascii: "-") && isDigit(at(i + 1))) {
                if c == UInt8(ascii: "-") {
                    emit(.number, i, i + 1)
                    i += 1
                }
                number()
            } else if isLetter(c) {
                while i < end, isIdent(b[i]) { i += 1 }
                if matches("true", at: start) || matches("false", at: start) || matches("null", at: start) { emit(.keyword, start, i) }
            } else {
                i += 1
            }
        }
    }

    // MARK: TOML

    mutating func toml() {
        var lineStart = true
        while i < end {
            let c = b[i]
            let start = i
            if c == 0x0A {
                lineStart = true
                i += 1
                continue
            }
            if c == 0x20 || c == 0x09 || c == 0x0D {
                i += 1
                continue
            }
            let atLineStart = lineStart
            lineStart = false
            if c == UInt8(ascii: "#") {
                skipLine()
                comment(start, i)
            } else if c == UInt8(ascii: "["), atLineStart {
                // A table header: its keys.
                while at(i) == UInt8(ascii: "[") { i += 1 }
                while i < end, b[i] != 0x0A, b[i] != UInt8(ascii: "]") {
                    if b[i] == UInt8(ascii: "\"") || b[i] == UInt8(ascii: "'") {
                        let q = i
                        i += 1
                        _ = scanString(quote: b[q], escapes: b[q] == UInt8(ascii: "\""), newlineEnds: true)
                        emit(.string, q, i)
                    } else if isIdent(b[i]) || b[i] == UInt8(ascii: "-") {
                        let w = i
                        while i < end, isIdent(b[i]) || b[i] == UInt8(ascii: "-") { i += 1 }
                        emit(.property, w, i)
                    } else {
                        i += 1
                    }
                }
                while i < end, b[i] == UInt8(ascii: "]") { i += 1 }
            } else if c == UInt8(ascii: "\"") || c == UInt8(ascii: "'") {
                let triple = at(i + 1) == c && at(i + 2) == c
                i += triple ? 3 : 1
                if triple {
                    while i < end, !(b[i] == c && at(i + 1) == c && at(i + 2) == c) {
                        i += c == UInt8(ascii: "\"") && b[i] == UInt8(ascii: "\\") ? 2 : 1
                    }
                    i = min(end, i + 3)
                    while at(i) == c { i += 1 }
                } else {
                    _ = scanString(quote: c, escapes: c == UInt8(ascii: "\""), newlineEnds: true)
                }
                var k = i
                while k < end, b[k] == 0x20 || b[k] == 0x09 { k += 1 }
                emit(!triple && at(k) == UInt8(ascii: "=") ? .property : .string, start, i)
            } else if isDigit(c) || ((c == UInt8(ascii: "-") || c == UInt8(ascii: "+")) && isDigit(at(i + 1))) {
                if !isDigit(c) { i += 1 }
                while i < end, isIdent(b[i]) || "-:.+".utf8.contains(b[i]) && isDigit(at(i + 1)) || b[i] == UInt8(ascii: ".") { i += 1 }
                emit(isTomlKey() ? .property : .number, start, i)
            } else if isIdent(c) || c == UInt8(ascii: "-") {
                while i < end, isIdent(b[i]) || b[i] == UInt8(ascii: "-") || b[i] == UInt8(ascii: ".") { i += 1 }
                if isTomlKey() {
                    emit(.property, start, i)
                } else if matches("true", at: start) || matches("false", at: start) {
                    emit(.keyword, start, i)
                } else if matches("inf", at: start) || matches("nan", at: start) {
                    emit(.number, start, i)
                }
            } else {
                i += 1
            }
        }
    }

    /// Whether `=` (not `==`) follows, so the word just read is a key.
    func isTomlKey() -> Bool {
        var k = i
        while k < end, b[k] == 0x20 || b[k] == 0x09 { k += 1 }
        return at(k) == UInt8(ascii: "=") && at(k + 1) != UInt8(ascii: "=")
    }

    // MARK: YAML

    mutating func yaml() {
        while i < end {
            yamlLine()
        }
    }

    mutating func yamlLine() {
        let lineStart = i
        while i < end, b[i] == 0x20 || b[i] == 0x09 { i += 1 }
        let indent = i - lineStart
        if at(i) == UInt8(ascii: "#") {
            let start = i
            skipLine()
            comment(start, i)
        } else if matches("---", at: i) || matches("...", at: i) {
            skipLine()
        } else {
            while at(i) == UInt8(ascii: "-") || at(i) == UInt8(ascii: "?"), at(i + 1) == 0x20 || at(i + 1) == 0x0A || i + 1 >= end {
                i += 1
                while i < end, b[i] == 0x20 { i += 1 }
            }
            yamlKey()
            yamlValue(indent: indent)
        }
        if i < end, b[i] == 0x0A { i += 1 }
    }

    /// A mapping key at `i`, if the line has one: up to a `:` that a space or the line's end follows.
    mutating func yamlKey() {
        let start = i
        var k = i
        if at(k) == UInt8(ascii: "\"") || at(k) == UInt8(ascii: "'") {
            let q = at(k)
            k += 1
            while k < end, b[k] != q, b[k] != 0x0A { k += b[k] == UInt8(ascii: "\\") && q == UInt8(ascii: "\"") ? 2 : 1 }
            k += 1
        } else {
            guard !"[{|>&*!%@`".utf8.contains(at(k)) else { return }
            while k < end, b[k] != 0x0A {
                if b[k] == UInt8(ascii: ":"), isSpace(at(k + 1)) || k + 1 >= end { break }
                if b[k] == UInt8(ascii: "#"), k > start, b[k - 1] == 0x20 { return }
                k += 1
            }
        }
        guard at(k) == UInt8(ascii: ":"), isSpace(at(k + 1)) || k + 1 >= end, k > start else { return }
        var e = k
        while e > start, b[e - 1] == 0x20 { e -= 1 }
        emit(.property, start, e)
        i = k + 1
    }

    mutating func yamlValue(indent: Int) {
        while i < end, b[i] == 0x20 || b[i] == 0x09 { i += 1 }
        while at(i) == UInt8(ascii: "&") || at(i) == UInt8(ascii: "*") || at(i) == UInt8(ascii: "!") {
            let start = i
            while i < end, !isSpace(b[i]) { i += 1 }
            emit(.type, start, i)
            while i < end, b[i] == 0x20 { i += 1 }
        }
        let start = i
        let c = at(i)
        if c == UInt8(ascii: "|") || c == UInt8(ascii: ">") {
            // A block scalar: every following line indented past the key.
            skipLine()
            while i < end {
                var k = i + 1
                var spaces = 0
                while k < end, b[k] == 0x20 { k += 1; spaces += 1 }
                if at(k) != 0x0A, spaces <= indent, k < end { break }
                if k >= end { break }
                i = k
                skipLine()
            }
            emit(.string, start + 1, i)
            return
        }
        if c == UInt8(ascii: "\"") || c == UInt8(ascii: "'") {
            i += 1
            while i < end, b[i] != c {
                i += c == UInt8(ascii: "\"") && b[i] == UInt8(ascii: "\\") ? 2 : 1
            }
            i = min(end, i + 1)
            emit(.string, start, i)
        } else if c == UInt8(ascii: "[") || c == UInt8(ascii: "{") {
            yamlFlow()
        } else if c != 0x0A, c != 0, c != UInt8(ascii: "#") {
            while i < end, b[i] != 0x0A, !(b[i] == UInt8(ascii: "#") && b[i - 1] == 0x20) { i += 1 }
            var e = i
            while e > start, b[e - 1] == 0x20 || b[e - 1] == 0x0D { e -= 1 }
            emit(yamlScalar(start, e), start, e)
        }
        while i < end, b[i] == 0x20 { i += 1 }
        if at(i) == UInt8(ascii: "#") {
            let c0 = i
            skipLine()
            comment(c0, i)
        } else {
            skipLine()
        }
    }

    func yamlScalar(_ start: Int, _ stop: Int) -> TokenKind {
        if yamlConstants.contains(b, start, stop) { return .keyword }
        var k = start
        if at(k) == UInt8(ascii: "-") || at(k) == UInt8(ascii: "+") { k += 1 }
        guard k < stop, isDigit(b[k]) || b[k] == UInt8(ascii: ".") else { return .string }
        for j in k..<stop where !(isDigit(b[j]) || "._xXoOeE+-abcdefABCDEF".utf8.contains(b[j])) { return .string }
        return .number
    }

    /// `[a, b]` and `{k: v}` on one line or many.
    mutating func yamlFlow() {
        var depth = 0
        while i < end {
            let c = b[i]
            let start = i
            if c == UInt8(ascii: "[") || c == UInt8(ascii: "{") {
                depth += 1
                i += 1
            } else if c == UInt8(ascii: "]") || c == UInt8(ascii: "}") {
                depth -= 1
                i += 1
                if depth == 0 { return }
            } else if c == UInt8(ascii: "\"") || c == UInt8(ascii: "'") {
                i += 1
                _ = scanString(quote: c, escapes: c == UInt8(ascii: "\""), newlineEnds: false)
                emit(.string, start, i)
            } else if c == UInt8(ascii: "#"), i > 0, isSpace(b[i - 1]) {
                skipLine()
                comment(start, i)
            } else if isSpace(c) || c == UInt8(ascii: ",") {
                i += 1
            } else {
                while i < end, !",[]{}\n".utf8.contains(b[i]), !(b[i] == UInt8(ascii: ":") && isSpace(at(i + 1))) { i += 1 }
                var e = i
                while e > start, b[e - 1] == 0x20 { e -= 1 }
                emit(at(i) == UInt8(ascii: ":") ? .property : yamlScalar(start, e), start, e)
                if at(i) == UInt8(ascii: ":") { i += 1 }
            }
        }
    }

    // MARK: CSS

    mutating func css() {
        var depth = 0
        var declaring = false
        var statementStart = true
        while i < end {
            let c = b[i]
            let start = i
            if isSpace(c) {
                i += 1
                continue
            }
            if c == UInt8(ascii: "/"), at(i + 1) == UInt8(ascii: "*") {
                blockComment(nested: false)
                comment(start, i)
                continue
            }
            if c == UInt8(ascii: "/"), at(i + 1) == UInt8(ascii: "/") {
                skipLine()
                comment(start, i)
                continue
            }
            if statementStart {
                statementStart = false
                // Inside a block, a statement that reaches `{` before `;` or `}` is a nested rule's selector.
                declaring = depth > 0 && !selectorAhead()
            }
            switch c {
            case UInt8(ascii: "{"):
                depth += 1
                i += 1
                statementStart = true
            case UInt8(ascii: "}"):
                depth = max(0, depth - 1)
                i += 1
                statementStart = true
            case UInt8(ascii: ";"):
                i += 1
                statementStart = true
            case UInt8(ascii: "\""), UInt8(ascii: "'"):
                i += 1
                _ = scanString(quote: c, escapes: true, newlineEnds: true)
                emit(.string, start, i)
            case UInt8(ascii: "@"):
                i += 1
                while i < end, isIdent(b[i]) || b[i] == UInt8(ascii: "-") { i += 1 }
                emit(.keyword, start, i)
            case UInt8(ascii: "!") where matches("!important", at: i):
                i += 10
                emit(.keyword, start, i)
            case _ where declaring:
                cssDeclaration()
            default:
                cssSelectorToken()
            }
        }
    }

    func selectorAhead() -> Bool {
        var k = i
        var parens = 0
        while k < end {
            let c = b[k]
            if c == UInt8(ascii: "(") { parens += 1 } else if c == UInt8(ascii: ")") { parens -= 1 } else if parens == 0 {
                if c == UInt8(ascii: "{") { return true }
                if c == UInt8(ascii: ";") || c == UInt8(ascii: "}") { return false }
            }
            if c == UInt8(ascii: "\"") || c == UInt8(ascii: "'") {
                k += 1
                while k < end, b[k] != c, b[k] != 0x0A { k += 1 }
            }
            k += 1
        }
        return false
    }

    /// One token of `name: value`.
    mutating func cssDeclaration() {
        let c = b[i]
        let start = i
        var k = i
        while k < end, isIdent(b[k]) || b[k] == UInt8(ascii: "-") { k += 1 }
        if k > i {
            var e = k
            while e < end, b[e] == 0x20 { e += 1 }
            let isName = at(e) == UInt8(ascii: ":") && !isPropertyValueContext()
            if isDigit(c) || (c == UInt8(ascii: "-") && (isDigit(at(i + 1)) || at(i + 1) == UInt8(ascii: "."))) {
                i += 1
                while i < end, isIdent(b[i]) || b[i] == UInt8(ascii: ".") || b[i] == UInt8(ascii: "%") { i += 1 }
                emit(.number, start, i)
                return
            }
            i = k
            if isName {
                emit(.property, start, i)
            } else if at(i) == UInt8(ascii: "(") {
                emit(.function, start, i)
                if matches("url", at: start), i - start == 3 {
                    let open = i + 1
                    while i < end, b[i] != UInt8(ascii: ")"), b[i] != 0x0A { i += 1 }
                    if at(open) != UInt8(ascii: "\""), at(open) != UInt8(ascii: "'") { emit(.string, open, i) } else { i = open }
                }
            }
            return
        }
        if c == UInt8(ascii: "#") {
            i += 1
            while i < end, isIdent(b[i]) { i += 1 }
            emit(.number, start, i)
            return
        }
        if c == UInt8(ascii: "."), isDigit(at(i + 1)) {
            i += 1
            while i < end, isIdent(b[i]) || b[i] == UInt8(ascii: "%") { i += 1 }
            emit(.number, start, i)
            return
        }
        i += 1
    }

    /// Whether a `:` was already seen in this declaration (so a word before another `:` is a value, as in `a:hover` inside a value).
    func isPropertyValueContext() -> Bool {
        var k = i - 1
        while k >= 0 {
            let c = b[k]
            if c == UInt8(ascii: ";") || c == UInt8(ascii: "{") || c == UInt8(ascii: "}") { return false }
            if c == UInt8(ascii: ":") { return true }
            k -= 1
        }
        return false
    }

    mutating func cssSelectorToken() {
        let c = b[i]
        let start = i
        switch c {
        case UInt8(ascii: "."), UInt8(ascii: "#"):
            i += 1
            while i < end, isIdent(b[i]) || b[i] == UInt8(ascii: "-") { i += 1 }
            emit(.property, start, i)
        case UInt8(ascii: ":"):
            i += 1
            if at(i) == UInt8(ascii: ":") { i += 1 }
            while i < end, isIdent(b[i]) || b[i] == UInt8(ascii: "-") { i += 1 }
            emit(.attribute, start, i)
        case UInt8(ascii: "["):
            i += 1
            let name = i
            while i < end, isIdent(b[i]) || b[i] == UInt8(ascii: "-") { i += 1 }
            emit(.attribute, name, i)
        case _ where isIdentStart(c) || c == UInt8(ascii: "-"):
            while i < end, isIdent(b[i]) || b[i] == UInt8(ascii: "-") { i += 1 }
            var e = i
            while e < end, b[e] == 0x20 { e += 1 }
            if at(e) == UInt8(ascii: ":"), isPreludeFeature() {
                emit(.property, start, i)
            } else if isDigit(c) {
                emit(.number, start, i)
            } else {
                emit(.keyword, start, i)
            }
        case _ where isDigit(c):
            while i < end, isIdent(b[i]) || b[i] == UInt8(ascii: ".") || b[i] == UInt8(ascii: "%") { i += 1 }
            emit(.number, start, i)
        default:
            i += 1
        }
    }

    /// Whether the word just read sits inside an at-rule's parentheses, as `max-width` does in `@media (max-width: 600px)`.
    func isPreludeFeature() -> Bool {
        var k = i - 1
        while k >= 0, b[k] != 0x0A {
            if b[k] == UInt8(ascii: "(") { return true }
            if b[k] == UInt8(ascii: ")") { return false }
            k -= 1
        }
        return false
    }
}
