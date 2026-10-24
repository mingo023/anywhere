/// Block math. The desktop parses `$$` fences as math (markdown-rs `math_flow`) and, having no math renderer, draws
/// them as code with no language. cmark has no math, so each `$$` fence becomes a backtick fence tagged `info`
/// before parsing, line for line, so source lines still match. Inline `$…$` needs nothing: cmark leaves it literal.
enum Math {
    /// The info string a rewritten math fence carries; a private-use character no real fence holds.
    static let info = "\u{E000}math"

    /// A rewritten line's new ending and the one it replaced, by 0-based line number. The rewrite can't tell indented
    /// code or HTML from a container's indent, so blocks that keep their source verbatim put these back.
    typealias Originals = [Int: (rewritten: String, original: String)]

    static func rewrite(_ source: String) -> (source: String, originals: Originals) {
        guard source.contains("$$") else { return (source, [:]) }
        var lines = source.split(omittingEmptySubsequences: false, whereSeparator: { $0 == "\n" || $0 == "\r\n" || $0 == "\r" }).map(String.init)
        var originals: Originals = [:]
        var fence: (ch: Character, len: Int)?
        var i = 0
        while i < lines.count {
            let (prefix, rest) = split(lines[i])
            if let open = fence {
                if SplitLine.closes(rest, open.ch, open.len) { fence = nil }
                i += 1
                continue
            }
            if let open = SplitLine.opens(rest) {
                fence = open
                i += 1
                continue
            }
            guard let dollars = opensMath(rest) else {
                i += 1
                continue
            }
            var close = i + 1
            var longest = 0
            while close < lines.count {
                let inner = split(lines[close]).rest
                if closesMath(inner, dollars) { break }
                longest = max(longest, longestRun(of: "`", in: inner))
                close += 1
            }
            let ticks = String(repeating: "`", count: max(3, longest + 1))
            originals[i] = (ticks + info, String(rest))
            lines[i] = prefix + ticks + info
            if close < lines.count {
                let (closePrefix, closeRest) = split(lines[close])
                originals[close] = (ticks, String(closeRest))
                lines[close] = closePrefix + ticks
            }
            i = close + 1
        }
        return (lines.joined(separator: "\n"), originals)
    }

    /// `literal`, a verbatim block starting on source line `start`, with any lines the rewrite changed put back.
    static func restore(_ literal: String, from start: Int, _ originals: Originals) -> String {
        guard !originals.isEmpty, literal.contains(Character("`")) else { return literal }
        var lines = literal.split(separator: "\n", omittingEmptySubsequences: false).map(String.init)
        for k in lines.indices {
            guard let line = originals[start + k], lines[k].hasSuffix(line.rewritten) else { continue }
            lines[k] = lines[k].dropLast(line.rewritten.count) + line.original
        }
        return lines.joined(separator: "\n")
    }

    /// A line's container prefix (indent and quote markers) and what follows it.
    static func split(_ line: String) -> (prefix: String, rest: Substring) {
        let rest = line.drop(while: { $0 == " " || $0 == "\t" || $0 == ">" })
        return (String(line[..<rest.startIndex]), rest)
    }

    static func opensMath(_ rest: Substring) -> Int? {
        let dollars = rest.prefix(while: { $0 == "$" }).count
        guard dollars >= 2, !rest.dropFirst(dollars).contains("$") else { return nil }
        return dollars
    }

    static func closesMath(_ rest: Substring, _ dollars: Int) -> Bool {
        let run = rest.prefix(while: { $0 == "$" }).count
        return run >= dollars && rest.dropFirst(run).allSatisfy(\.isWhitespace)
    }

    static func longestRun(of ch: Character, in s: Substring) -> Int {
        var best = 0
        var run = 0
        for c in s {
            run = c == ch ? run + 1 : 0
            best = max(best, run)
        }
        return best
    }
}

/// Code-fence lines with any container prefix already taken off.
enum SplitLine {
    static func opens(_ rest: Substring) -> (ch: Character, len: Int)? {
        guard let ch = rest.first, ch == "`" || ch == "~" else { return nil }
        let len = rest.prefix(while: { $0 == ch }).count
        guard len >= 3, !(ch == "`" && rest.dropFirst(len).contains("`")) else { return nil }
        return (ch, len)
    }

    static func closes(_ rest: Substring, _ ch: Character, _ len: Int) -> Bool {
        let run = rest.prefix(while: { $0 == ch }).count
        return run >= len && rest.dropFirst(run).allSatisfy(\.isWhitespace)
    }
}
