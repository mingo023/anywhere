enum ShellFlavor { case bash, dockerfile, makefile }

private let shellKeywords = Words("if then else elif fi for while until do done case esac in function select time coproc return exit local export declare readonly typeset unset shift source alias break continue eval exec trap")
/// Keywords after which the next word starts a command.
private let shellCommandKeywords = Words("if then else elif do while until time ! exec eval")
private let dockerInstructions = Words("FROM RUN CMD LABEL MAINTAINER EXPOSE ENV ADD COPY ENTRYPOINT VOLUME USER WORKDIR ARG ONBUILD STOPSIGNAL HEALTHCHECK SHELL AS")
private let dockerShellInstructions = Words("RUN CMD ENTRYPOINT SHELL")
private let makeDirectives = Words("include -include sinclude ifeq ifneq ifdef ifndef else endif define endef export unexport override vpath")

extension Lexer {
    struct ShellState {
        var command = true
        var lineStart = true
        var lastKeyword: Word?
        var heredocs: [(delimiter: [UInt8], dash: Bool)] = []
    }

    mutating func shell(_ flavor: ShellFlavor) {
        var s = ShellState()
        shellCode(flavor, &s, until: 0)
    }

    /// Lexes shell until `close` (left unconsumed) or the end.
    mutating func shellCode(_ flavor: ShellFlavor, _ s: inout ShellState, until close: UInt8) {
        var depth = 0
        while i < end {
            let c = b[i]
            if s.lineStart {
                s.lineStart = false
                if lineStart(flavor, &s) { continue }
            }
            let start = i
            switch c {
            case 0x0A:
                i += 1
                if !s.heredocs.isEmpty { heredocBodies(&s) }
                s.command = true
                s.lineStart = true
            case 0x20, 0x09, 0x0D:
                i += 1
            case UInt8(ascii: "\\"):
                // The escaped character may be several bytes; a word mustn't start inside it.
                i += 1
                if i < end { skipScalar() }
            case UInt8(ascii: "#") where start == 0 || isSpace(at(start - 1)) || ";|&(".utf8.contains(at(start - 1)):
                skipLine()
                comment(start, i)
            case UInt8(ascii: "'"):
                i += 1
                _ = scanString(quote: c, escapes: false, newlineEnds: false)
                emit(.string, start, i)
                // A value being assigned leaves the command still to come.
                if at(start - 1) != UInt8(ascii: "=") { s.command = false }
            case UInt8(ascii: "\""):
                doubleQuoted(flavor)
                if at(start - 1) != UInt8(ascii: "=") { s.command = false }
            case UInt8(ascii: "$"):
                expansion(flavor)
                if at(start - 1) != UInt8(ascii: "=") { s.command = false }
            case UInt8(ascii: "`"):
                i += 1
                var inner = ShellState()
                shellCode(flavor, &inner, until: UInt8(ascii: "`"))
                if i < end { i += 1 }
                s.command = false
            case UInt8(ascii: "<") where at(i + 1) == UInt8(ascii: "<") && at(i + 2) != UInt8(ascii: "<"):
                heredocOperator(&s)
            case UInt8(ascii: ";"), UInt8(ascii: "|"), UInt8(ascii: "&"), UInt8(ascii: "("), UInt8(ascii: "{"):
                if c == close, depth == 0 { return }
                if c == UInt8(ascii: "(") && close == UInt8(ascii: ")") { depth += 1 }
                i += 1
                s.command = true
            case UInt8(ascii: ")"), UInt8(ascii: "}"):
                if c == close, depth == 0 { return }
                if c == UInt8(ascii: ")") && close == UInt8(ascii: ")") { depth -= 1 }
                i += 1
                s.command = c == UInt8(ascii: ")")
            case UInt8(ascii: "<"), UInt8(ascii: ">"), UInt8(ascii: "!"):
                i += 1
            default:
                if c == close, depth == 0 { return }
                shellWord(flavor, &s)
            }
        }
    }

    /// A line's opening in Dockerfiles and Makefiles, and a console prompt; true when it took something.
    mutating func lineStart(_ flavor: ShellFlavor, _ s: inout ShellState) -> Bool {
        switch flavor {
        case .bash:
            if b[i] == UInt8(ascii: "$"), at(i + 1) == 0x20 {
                i += 2
                s.command = true
                return true
            }
            return false
        case .dockerfile:
            var k = i
            while k < end, b[k] == 0x20 || b[k] == 0x09 { k += 1 }
            let start = k
            while k < end, isLetter(b[k]) { k += 1 }
            guard k > start, dockerInstructions.contains(b, start, k) else { return false }
            emit(.keyword, start, k)
            i = k
            s.command = dockerShellInstructions.contains(b, start, k)
            return true
        case .makefile:
            if b[i] == 0x09 {
                // A recipe line: a command, perhaps quieted with `@` or let fail with `-`.
                i += 1
                while i < end, b[i] == UInt8(ascii: "@") || b[i] == UInt8(ascii: "-") || b[i] == UInt8(ascii: "+") { i += 1 }
                s.command = true
                return true
            }
            var k = i
            while k < end, b[k] == 0x20 { k += 1 }
            let start = k
            while k < end, !isSpace(b[k]), b[k] != UInt8(ascii: ":"), b[k] != UInt8(ascii: "="), b[k] != UInt8(ascii: "#"), b[k] != UInt8(ascii: "?"), b[k] != UInt8(ascii: "+"), b[k] != UInt8(ascii: "$") { k += 1 }
            guard k > start else { return false }
            if makeDirectives.contains(b, start, k) {
                emit(.keyword, start, k)
                i = k
                s.command = false
                return true
            }
            var e = k
            while e < end, b[e] == 0x20 || b[e] == 0x09 { e += 1 }
            if at(e) == UInt8(ascii: "=") || ("?+!".utf8.contains(at(e)) && at(e + 1) == UInt8(ascii: "=")) {
                emit(.property, start, k)
                i = k
                s.command = false
                return true
            }
            if at(e) == UInt8(ascii: ":") {
                if at(e + 1) == UInt8(ascii: "=") || (at(e + 1) == UInt8(ascii: ":") && at(e + 2) == UInt8(ascii: "=")) {
                    emit(.property, start, k)
                } else {
                    // Targets: every word up to the colon.
                    var t = start
                    while t < e {
                        let w = t
                        while t < e, !isSpace(b[t]) { t += 1 }
                        emit(b[w] == UInt8(ascii: ".") ? .keyword : .function, w, t)
                        while t < e, isSpace(b[t]) { t += 1 }
                    }
                }
                i = e
                s.command = false
                return true
            }
            return false
        }
    }

    mutating func shellWord(_ flavor: ShellFlavor, _ s: inout ShellState) {
        let start = i
        while i < end, !isSpace(b[i]), !";|&()<>\"'`$".utf8.contains(b[i]), !(b[i] == UInt8(ascii: "}") && i > start) { i += 1 }
        if i == start { i += 1 }
        // `NAME=value` sets a variable and leaves the command still to come.
        if s.command, let eq = (start..<i).first(where: { b[$0] == UInt8(ascii: "=") }), eq > start, isIdentStart(b[start]),
            (start..<eq).allSatisfy({ isIdent(b[$0]) && b[$0] != UInt8(ascii: "$") }) {
            emit(.property, start, eq)
            return
        }
        let word = Word(b, start, i)
        // `in` is a keyword only where `for`, `case` or `select` expect it.
        let expectedIn = word == Word("in") && s.lastKeyword.map { [Word("for"), Word("case"), Word("select")].contains($0) } == true
        if shellKeywords.contains(b, start, i), s.command || expectedIn {
            emit(.keyword, start, i)
            s.lastKeyword = word
            s.command = shellCommandKeywords.contains(b, start, i)
            return
        }
        if flavor == .dockerfile, word == Word("AS") {
            emit(.keyword, start, i)
            return
        }
        // `[ … ]` and `[[ … ]]` test; they name no command.
        if word == Word("[") || word == Word("[[") {
            s.command = false
            return
        }
        if s.command {
            emit(.function, start, i)
            s.command = false
            return
        }
        if (start..<i).allSatisfy({ isDigit(b[$0]) }) { emit(.number, start, i) }
    }

    /// `$name`, `$1`, `${…}` as variables, `$(…)` and `` `…` `` as commands, `$'…'` as a string.
    mutating func expansion(_ flavor: ShellFlavor) {
        let start = i
        let next = at(i + 1)
        if next == UInt8(ascii: "'") {
            i += 2
            _ = scanString(quote: next, escapes: true, newlineEnds: false)
            emit(.string, start, i)
        } else if next == UInt8(ascii: "(") && flavor != .makefile {
            i += 2
            var inner = ShellState()
            shellCode(flavor, &inner, until: UInt8(ascii: ")"))
            if i < end { i += 1 }
        } else if next == UInt8(ascii: "{") || next == UInt8(ascii: "(") {
            let close = next == UInt8(ascii: "{") ? UInt8(ascii: "}") : UInt8(ascii: ")")
            i += 2
            var depth = 0
            while i < end {
                if b[i] == next { depth += 1 } else if b[i] == close {
                    if depth == 0 { break }
                    depth -= 1
                }
                i += 1
            }
            if i < end { i += 1 }
            emit(.property, start, i)
        } else if isIdentStart(next) {
            i += 1
            while i < end, isIdent(b[i]), b[i] != UInt8(ascii: "$") { i += 1 }
            emit(.property, start, i)
        } else if isDigit(next) || "?#@*!-$<^%+|".utf8.contains(next) {
            i += 2
            emit(.property, start, i)
        } else {
            i += 1
        }
    }

    mutating func doubleQuoted(_ flavor: ShellFlavor) {
        var from = i
        i += 1
        while i < end {
            let c = b[i]
            if c == UInt8(ascii: "\"") {
                i += 1
                break
            }
            if c == UInt8(ascii: "\\") {
                i += 2
                continue
            }
            if c == UInt8(ascii: "$"), isIdentStart(at(i + 1)) || "{(?#@*!0123456789".utf8.contains(at(i + 1)) {
                emit(.string, from, i)
                expansion(flavor)
                from = i
                continue
            }
            if c == UInt8(ascii: "`") {
                emit(.string, from, i)
                i += 1
                var inner = ShellState()
                shellCode(flavor, &inner, until: UInt8(ascii: "`"))
                if i < end { i += 1 }
                from = i
                continue
            }
            i += 1
        }
        emit(.string, from, min(i, end))
    }

    /// `<<EOF`, `<<-EOF`, `<<'EOF'`: the delimiter as a string, and its body to read from the next line.
    mutating func heredocOperator(_ s: inout ShellState) {
        i += 2
        let dash = at(i) == UInt8(ascii: "-")
        if dash { i += 1 }
        while i < end, b[i] == 0x20 { i += 1 }
        let start = i
        let quote = at(i) == UInt8(ascii: "'") || at(i) == UInt8(ascii: "\"") ? at(i) : 0
        if quote != 0 { i += 1 }
        let word = i
        while i < end, isIdent(b[i]) || b[i] == UInt8(ascii: "-") || b[i] == UInt8(ascii: ".") { i += 1 }
        let delimiter = Array(b[word..<i])
        if quote != 0, at(i) == quote { i += 1 }
        emit(.string, start, i)
        if !delimiter.isEmpty { s.heredocs.append((delimiter, dash)) }
    }

    /// The bodies of the heredocs opened on the line just ended, each through its delimiter line.
    mutating func heredocBodies(_ s: inout ShellState) {
        for doc in s.heredocs {
            let start = i
            while i < end {
                let line = i
                while i < end, b[i] != 0x0A { i += 1 }
                var k = line
                if doc.dash { while k < i, b[k] == 0x09 { k += 1 } }
                var e = i
                while e > k, b[e - 1] == 0x0D || b[e - 1] == 0x20 { e -= 1 }
                let isEnd = e - k == doc.delimiter.count && zip(k..<e, doc.delimiter).allSatisfy { b[$0] == $1 }
                if isEnd { break }
                if i < end { i += 1 }
            }
            emit(.string, start, i)
            if i < end { i += 1 }
        }
        s.heredocs = []
    }
}
