/// What sets one curly-brace language apart from another.
struct CSpec: Sendable {
    enum Strings: Sendable { case c, rust, swift, go, js, python, kotlin, java }

    var keywords: Words
    /// Built-in lowercase types: `i32`, `string`, `int`.
    var types = Words("")
    /// Keywords whose next word names a function, and those whose next word names a type.
    var functionDecl = Words("")
    var typeDecl = Words("")
    var strings: Strings
    var slashComments = true
    var hashComments = false
    var nestedComments = false
    /// `@name` is an attribute, annotation or decorator.
    var annotations = false
    /// `#[derive(…)]` is an attribute (Rust).
    var hashAttributes = false
    /// `#if`, `#available` are directives (Swift).
    var hashDirectives = false
    var regex = false
    var jsx = false
    /// `SCREAMING_CASE` names a constant.
    var screamingConstants = true
    /// `name!(…)` is a macro call (Rust).
    var macros = false
    /// `{ key: value }` keys are properties (JavaScript).
    var objectKeys = false
}

extension Lexer {
    enum Decl { case none, function, type }

    /// State of a C-family lex: what came before the current token.
    struct CState {
        var prev: UInt8 = 0
        var prevAt = -1
        var regexOK = true
        var pending = Decl.none
        var brackets: [UInt8] = []
        var lineStart = true
    }

    mutating func clike(_ spec: CSpec) {
        var state = CState()
        code(spec, &state, until: 0)
    }

    /// Lexes code until `close` at depth 0 (left unconsumed), or the end.
    mutating func code(_ spec: CSpec, _ s: inout CState, until close: UInt8) {
        let open: UInt8 = close == UInt8(ascii: "}") ? UInt8(ascii: "{") : close == UInt8(ascii: ")") ? UInt8(ascii: "(") : 0
        var depth = 0
        while i < end {
            let c = b[i]
            if isSpace(c) {
                if c == 0x0A { s.lineStart = true }
                i += 1
                continue
            }
            let start = i
            let wasLineStart = s.lineStart
            s.lineStart = false
            switch c {
            case UInt8(ascii: "/") where spec.slashComments && at(i + 1) == UInt8(ascii: "/"):
                skipLine()
                comment(start, i)
                continue
            case UInt8(ascii: "/") where spec.slashComments && at(i + 1) == UInt8(ascii: "*"):
                blockComment(nested: spec.nestedComments)
                comment(start, i)
                continue
            case UInt8(ascii: "#") where spec.hashComments:
                skipLine()
                comment(start, i)
                continue
            case UInt8(ascii: "/") where spec.regex && s.regexOK:
                if regexLiteral() {
                    emit(.string, start, i)
                    s.regexOK = false
                    s.prev = UInt8(ascii: "/")
                    continue
                }
                i += 1
            case UInt8(ascii: "#") where spec.hashAttributes && (at(i + 1) == UInt8(ascii: "[") || (at(i + 1) == UInt8(ascii: "!") && at(i + 2) == UInt8(ascii: "["))):
                hashAttribute()
                emit(.attribute, start, i)
                s.regexOK = true
                continue
            case UInt8(ascii: "#") where spec.strings == .swift && swiftRawString():
                emit(.string, start, i)
                s.regexOK = false
                s.prev = UInt8(ascii: "\"")
                continue
            case UInt8(ascii: "#") where spec.hashDirectives && isIdentStart(at(i + 1)):
                i += 1
                while i < end, isIdent(b[i]) { i += 1 }
                emit(.preproc, start, i)
                continue
            case UInt8(ascii: "\""), UInt8(ascii: "'"), UInt8(ascii: "`"):
                if quote(spec, &s, c) {
                    s.regexOK = false
                    s.prev = c
                    s.pending = .none
                    continue
                }
                i += 1
            case UInt8(ascii: "@") where spec.annotations && isIdentStart(at(i + 1)) && (spec.strings != .python || wasLineStart):
                i += 1
                while i < end, isIdent(b[i]) || (b[i] == UInt8(ascii: ".") && isIdentStart(at(i + 1))) { i += 1 }
                emit(.attribute, start, i)
                s.regexOK = true
                continue
            case UInt8(ascii: "<") where spec.jsx && s.regexOK && opensJSX():
                jsxElement(spec)
                s.regexOK = false
                s.prev = UInt8(ascii: ">")
                continue
            case _ where isDigit(c) || (c == UInt8(ascii: ".") && isDigit(at(i + 1)) && s.prev != UInt8(ascii: ".")):
                number()
                s.regexOK = false
                s.prev = UInt8(ascii: "0")
                s.prevAt = i - 1
                s.pending = .none
                continue
            case _ where isIdentStart(c) || c == UInt8(ascii: "$"):
                if prefixedString(spec, &s) {
                    s.regexOK = false
                    s.prev = UInt8(ascii: "\"")
                    continue
                }
                ident(spec, &s)
                continue
            default:
                if c == close, depth == 0 { return }
                if c == open { depth += 1 } else if c == close { depth -= 1 }
                if c == UInt8(ascii: "(") || c == UInt8(ascii: "[") || c == UInt8(ascii: "{") {
                    s.brackets.append(c)
                } else if c == UInt8(ascii: ")") || c == UInt8(ascii: "]") || c == UInt8(ascii: "}") {
                    _ = s.brackets.popLast()
                }
                // A word after `)`, `]` or `}` divides; after any other mark a `/` opens a regex.
                s.regexOK = !(c == UInt8(ascii: ")") || c == UInt8(ascii: "]") || c == UInt8(ascii: "}"))
                if c != UInt8(ascii: "<") && c != UInt8(ascii: ">") && c != UInt8(ascii: ",") { s.pending = .none }
                i += 1
            }
            s.prev = c
            s.prevAt = start
        }
    }

    mutating func blockComment(nested: Bool) {
        i += 2
        var depth = 1
        while i < end {
            if b[i] == UInt8(ascii: "*"), at(i + 1) == UInt8(ascii: "/") {
                i += 2
                depth -= 1
                if depth == 0 || !nested { return }
            } else if nested, b[i] == UInt8(ascii: "/"), at(i + 1) == UInt8(ascii: "*") {
                i += 2
                depth += 1
            } else {
                i += 1
            }
        }
    }

    mutating func ident(_ spec: CSpec, _ s: inout CState) {
        let start = i
        while i < end, isIdent(b[i]) { i += 1 }
        let afterDot = s.prev == UInt8(ascii: ".") && at(s.prevAt - 1) != UInt8(ascii: ".")
        defer {
            s.prev = b[i - 1]
            s.prevAt = i - 1
        }
        if !afterDot, spec.keywords.contains(b, start, i) {
            emit(.keyword, start, i)
            s.pending = spec.functionDecl.contains(b, start, i) ? .function : spec.typeDecl.contains(b, start, i) ? .type : .none
            s.regexOK = true
            return
        }
        s.regexOK = false
        var next = i
        while next < end, b[next] == 0x20 || b[next] == 0x09 { next += 1 }
        let after = at(next)
        let kind: TokenKind
        if spec.macros, at(i) == UInt8(ascii: "!"), at(i + 1) != UInt8(ascii: "=") {
            i += 1
            kind = .function
        } else if s.pending == .function || after == UInt8(ascii: "(") {
            kind = s.pending == .type ? .type : .function
        } else if s.pending == .type {
            kind = .type
        } else if spec.screamingConstants, isConstant(start, i) {
            kind = .property
        } else if isUpper(b[start]) {
            kind = .type
        } else if afterDot {
            kind = .property
        } else if spec.objectKeys, after == UInt8(ascii: ":"), at(next + 1) != UInt8(ascii: ":"), s.prev == UInt8(ascii: "{") || s.prev == UInt8(ascii: ","), s.brackets.last == UInt8(ascii: "{") {
            kind = .property
        } else if spec.types.contains(b, start, i) {
            kind = .type
        } else {
            kind = .plain
        }
        s.pending = .none
        emit(kind, start, i)
    }

    func isConstant(_ start: Int, _ stop: Int) -> Bool {
        guard stop - start >= 2, isUpper(b[start]) else { return false }
        for k in start..<stop where !(isUpper(b[k]) || isDigit(b[k]) || b[k] == UInt8(ascii: "_")) { return false }
        return true
    }

    /// `#[…]` or `#![…]` through its closing bracket, strings inside included.
    mutating func hashAttribute() {
        while i < end, b[i] != UInt8(ascii: "[") { i += 1 }
        var depth = 0
        while i < end {
            let c = b[i]
            if c == UInt8(ascii: "\"") {
                i += 1
                _ = scanString(quote: c, escapes: true, newlineEnds: false)
                continue
            }
            if c == UInt8(ascii: "[") { depth += 1 } else if c == UInt8(ascii: "]") {
                depth -= 1
                if depth == 0 {
                    i += 1
                    return
                }
            }
            i += 1
        }
    }

    /// Scans to just past the closing `quote`, returning false when the string ran out unclosed.
    mutating func scanString(quote: UInt8, escapes: Bool, newlineEnds: Bool) -> Bool {
        while i < end {
            let c = b[i]
            if c == quote {
                i += 1
                return true
            }
            if escapes, c == UInt8(ascii: "\\") {
                i += 1
                if i < end { skipScalar() }
                continue
            }
            if newlineEnds, c == 0x0A { return false }
            i += 1
        }
        return false
    }

    /// A regex literal at `/`, through its flags; false (and `i` unmoved) if the line ends first.
    mutating func regexLiteral() -> Bool {
        let start = i
        i += 1
        var inClass = false
        while i < end {
            let c = b[i]
            if c == 0x0A { break }
            if c == UInt8(ascii: "\\") {
                i += 2
                continue
            }
            if c == UInt8(ascii: "[") { inClass = true } else if c == UInt8(ascii: "]") { inClass = false } else if c == UInt8(ascii: "/"), !inClass {
                i += 1
                while i < end, isLetter(b[i]) { i += 1 }
                return true
            }
            i += 1
        }
        i = start
        return false
    }

    /// Swift `#"…"#` and `#"""…"""#` at `#`; false (and `i` unmoved) for any other `#`.
    mutating func swiftRawString() -> Bool {
        var k = i
        while at(k) == UInt8(ascii: "#") { k += 1 }
        guard at(k) == UInt8(ascii: "\"") else { return false }
        let hashes = k - i
        let triple = at(k + 1) == UInt8(ascii: "\"") && at(k + 2) == UInt8(ascii: "\"")
        i = k + (triple ? 3 : 1)
        while i < end {
            if b[i] == UInt8(ascii: "\""), !triple || (at(i + 1) == UInt8(ascii: "\"") && at(i + 2) == UInt8(ascii: "\"")) {
                let q = i + (triple ? 3 : 1)
                var h = 0
                while h < hashes, at(q + h) == UInt8(ascii: "#") { h += 1 }
                if h == hashes {
                    i = q + h
                    return true
                }
            }
            if !triple, b[i] == 0x0A { return true }
            i += 1
        }
        return true
    }

    /// A string at a quote, by the language's rules; false when `c` opens no string here (a Rust lifetime).
    mutating func quote(_ spec: CSpec, _ s: inout CState, _ c: UInt8) -> Bool {
        let start = i
        let triple = at(i + 1) == c && at(i + 2) == c
        switch (spec.strings, c) {
        case (.rust, UInt8(ascii: "'")), (.go, UInt8(ascii: "'")), (.java, UInt8(ascii: "'")), (.kotlin, UInt8(ascii: "'")), (.c, UInt8(ascii: "'")):
            // A character, unless it's a Rust lifetime: `'a` with no closing quote right after one character.
            i += 1
            if at(i) == UInt8(ascii: "\\") {
                _ = scanString(quote: c, escapes: true, newlineEnds: true)
            } else {
                let ch = i
                if i < end { skipScalar() }
                if at(i) == c {
                    i += 1
                } else if spec.strings == .rust, isIdentStart(at(ch)) {
                    while i < end, isIdent(b[i]) { i += 1 }
                    s.prev = UInt8(ascii: "a")
                    return true
                } else {
                    _ = scanString(quote: c, escapes: true, newlineEnds: true)
                }
            }
            emit(.string, start, i)
        case (.go, UInt8(ascii: "`")):
            i += 1
            _ = scanString(quote: c, escapes: false, newlineEnds: false)
            emit(.string, start, i)
        case (.js, UInt8(ascii: "`")):
            i += 1
            interpolated(spec, from: start, quote: c, triple: false, escapes: true, newlineEnds: false, opener: .dollarBrace)
        case (.python, _):
            pythonString(spec, from: start, prefix: "", quote: c)
        case (.swift, UInt8(ascii: "\"")), (.kotlin, UInt8(ascii: "\"")), (.java, UInt8(ascii: "\"")):
            i += triple ? 3 : 1
            let opener: Interpolation = spec.strings == .swift ? .backslashParen : spec.strings == .kotlin ? .dollar : .none
            interpolated(spec, from: start, quote: c, triple: triple, escapes: !(spec.strings == .kotlin && triple), newlineEnds: !triple, opener: opener)
        case (_, UInt8(ascii: "`")), (.swift, UInt8(ascii: "'")):
            return false
        default:
            i += 1
            _ = scanString(quote: c, escapes: true, newlineEnds: spec.strings != .rust)
            emit(.string, start, i)
        }
        return true
    }

    enum Interpolation { case none, dollarBrace, backslashParen, dollar, brace }

    /// A string with code inside: `${…}`, `\(…)`, `$name` or `{…}`. Code inside reads as code; the rest is string.
    mutating func interpolated(_ spec: CSpec, from start: Int, quote: UInt8, triple: Bool, escapes: Bool, newlineEnds: Bool, opener: Interpolation) {
        var from = start
        while i < end {
            let c = b[i]
            if c == quote, !triple || (at(i + 1) == quote && at(i + 2) == quote) {
                i += triple ? 3 : 1
                // A Kotlin or Swift raw `"""` ends at the last of a run of quotes.
                while triple, at(i) == quote { i += 1 }
                break
            }
            if newlineEnds, c == 0x0A { break }
            let inner: (skip: Int, close: UInt8)?
            switch opener {
            case .dollarBrace where c == UInt8(ascii: "$") && at(i + 1) == UInt8(ascii: "{"): inner = (2, UInt8(ascii: "}"))
            case .dollar where c == UInt8(ascii: "$") && at(i + 1) == UInt8(ascii: "{"): inner = (2, UInt8(ascii: "}"))
            case .backslashParen where c == UInt8(ascii: "\\") && at(i + 1) == UInt8(ascii: "("): inner = (2, UInt8(ascii: ")"))
            case .brace where c == UInt8(ascii: "{") && at(i + 1) == UInt8(ascii: "{"):
                i += 2
                continue
            case .brace where c == UInt8(ascii: "{"): inner = (1, UInt8(ascii: "}"))
            default: inner = nil
            }
            if let (skip, close) = inner {
                emit(.string, from, i)
                i += skip
                var state = CState()
                code(spec, &state, until: close)
                if i < end { i += 1 }
                from = i
                continue
            }
            if opener == .dollar, c == UInt8(ascii: "$"), isIdentStart(at(i + 1)) {
                emit(.string, from, i)
                i += 1
                while i < end, isIdent(b[i]), b[i] != UInt8(ascii: "$") { i += 1 }
                from = i
                continue
            }
            if escapes, c == UInt8(ascii: "\\") {
                i += 1
                if i < end { skipScalar() }
                continue
            }
            i += 1
        }
        emit(.string, from, i)
    }

    /// A string whose prefix is a word: Rust `r#"…"#`, `b"…"`, `br"…"`, Python `f"…"`, `rb'…'`.
    mutating func prefixedString(_ spec: CSpec, _ s: inout CState) -> Bool {
        let start = i
        switch spec.strings {
        case .rust:
            var k = i
            if at(k) == UInt8(ascii: "b") || at(k) == UInt8(ascii: "c") { k += 1 }
            let raw = at(k) == UInt8(ascii: "r")
            if raw { k += 1 }
            var hashes = 0
            while raw, at(k + hashes) == UInt8(ascii: "#") { hashes += 1 }
            let q = at(k + hashes)
            guard k > i, q == UInt8(ascii: "\"") || (q == UInt8(ascii: "'") && !raw && hashes == 0 && at(i) == UInt8(ascii: "b")) else {
                // `r#type`: a raw identifier.
                if raw, hashes == 1, isIdentStart(q), k == i + 1 {
                    i = k + 1
                    while i < end, isIdent(b[i]) { i += 1 }
                    emit(.plain, start, i)
                    return true
                }
                return false
            }
            i = k + hashes + 1
            if q == UInt8(ascii: "'") {
                _ = scanString(quote: q, escapes: true, newlineEnds: true)
            } else if raw {
                while i < end {
                    if b[i] == q {
                        var h = 0
                        while h < hashes, at(i + 1 + h) == UInt8(ascii: "#") { h += 1 }
                        if h == hashes {
                            i += 1 + hashes
                            break
                        }
                    }
                    i += 1
                }
            } else {
                _ = scanString(quote: q, escapes: true, newlineEnds: false)
            }
            emit(.string, start, i)
            return true
        case .python:
            var k = i
            while k - i < 3, "rRbBuUfF".utf8.contains(at(k)) { k += 1 }
            let q = at(k)
            guard k > i, q == UInt8(ascii: "\"") || q == UInt8(ascii: "'") else { return false }
            let prefix = String(decoding: UnsafeBufferPointer(rebasing: b[i..<k]), as: UTF8.self).lowercased()
            i = k
            pythonString(spec, from: start, prefix: prefix, quote: q)
            return true
        default:
            return false
        }
    }

    mutating func pythonString(_ spec: CSpec, from start: Int, prefix: String, quote q: UInt8) {
        let triple = at(i + 1) == q && at(i + 2) == q
        i += triple ? 3 : 1
        let opener: Interpolation = prefix.contains("f") ? .brace : .none
        interpolated(spec, from: start, quote: q, triple: triple, escapes: true, newlineEnds: !triple, opener: opener)
    }

    // MARK: JSX

    /// A JSX element at `<`: its tags as tags, attribute names as attributes, `{…}` as code, the rest as text.
    mutating func jsxElement(_ spec: CSpec) {
        i += 1
        if at(i) == UInt8(ascii: ">") {
            i += 1
            jsxChildren(spec)
            return
        }
        jsxName()
        while i < end {
            let c = b[i]
            if isSpace(c) {
                i += 1
            } else if c == UInt8(ascii: "/"), at(i + 1) == UInt8(ascii: ">") {
                i += 2
                return
            } else if c == UInt8(ascii: ">") {
                i += 1
                jsxChildren(spec)
                return
            } else if c == UInt8(ascii: "{") {
                i += 1
                var state = CState()
                code(spec, &state, until: UInt8(ascii: "}"))
                if i < end { i += 1 }
            } else if isIdentStart(c) {
                let start = i
                while i < end, isIdent(b[i]) || b[i] == UInt8(ascii: "-") || b[i] == UInt8(ascii: ":") { i += 1 }
                emit(.attribute, start, i)
            } else if c == UInt8(ascii: "\"") || c == UInt8(ascii: "'") {
                let start = i
                i += 1
                _ = scanString(quote: c, escapes: false, newlineEnds: false)
                emit(.string, start, i)
            } else {
                i += 1
            }
        }
    }

    /// Whether the `<` at `i` opens a JSX element rather than a type parameter list (`<T,>`, `<T extends U>`).
    func opensJSX() -> Bool {
        if at(i + 1) == UInt8(ascii: ">") { return true }
        guard isIdentStart(at(i + 1)) else { return false }
        var k = i + 1
        while k < end, isIdent(b[k]) || b[k] == UInt8(ascii: ".") || b[k] == UInt8(ascii: "-") || b[k] == UInt8(ascii: ":") { k += 1 }
        let next = at(k)
        guard isSpace(next) || next == UInt8(ascii: ">") || next == UInt8(ascii: "/") else { return false }
        while k < end, isSpace(b[k]) { k += 1 }
        return !matches("extends ", at: k)
    }

    mutating func jsxName() {
        let start = i
        while i < end, isIdent(b[i]) || b[i] == UInt8(ascii: ".") || b[i] == UInt8(ascii: "-") || b[i] == UInt8(ascii: ":") { i += 1 }
        emit(isUpper(at(start)) ? .type : .keyword, start, i)
    }

    mutating func jsxChildren(_ spec: CSpec) {
        while i < end {
            let c = b[i]
            if c == UInt8(ascii: "<"), at(i + 1) == UInt8(ascii: "/") {
                i += 2
                jsxName()
                while i < end, b[i] != UInt8(ascii: ">") { i += 1 }
                if i < end { i += 1 }
                return
            } else if c == UInt8(ascii: "<"), isIdentStart(at(i + 1)) || at(i + 1) == UInt8(ascii: ">") {
                jsxElement(spec)
            } else if c == UInt8(ascii: "{") {
                i += 1
                var state = CState()
                code(spec, &state, until: UInt8(ascii: "}"))
                if i < end { i += 1 }
            } else {
                i += 1
            }
        }
    }
}
