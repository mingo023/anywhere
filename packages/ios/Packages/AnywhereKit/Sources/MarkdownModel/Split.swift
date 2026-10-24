/// The chat's own reading of a message's shape (desktop `chat/src/rows.rs markdown_blocks`, `pocket/chat/markdown.rs fence`).
public enum MarkdownSplit {
    /// Ranges of the top-level blocks of `text`, never splitting a fenced code block.
    /// Blank lines end a block unless a list carries on past them; a heading always ends its block;
    /// an unclosed fence runs to the end.
    public static func blocks(_ text: String) -> [Range<String.Index>] {
        var blocks: [Range<String.Index>] = []
        var current: Range<String.Index>?
        var fence: (ch: Character, len: Int)?
        var list = false
        var afterBlank = false
        var lineStart = text.startIndex
        while lineStart < text.endIndex {
            // "\r\n" is one Character in Swift, not a "\n".
            let newline = text[lineStart...].firstIndex(where: { $0 == "\n" || $0 == "\r\n" })
            var end = newline ?? text.endIndex
            while end > lineStart, text[text.index(before: end)] == "\r" { end = text.index(before: end) }
            let range = lineStart..<end
            lineStart = newline.map { text.index(after: $0) } ?? text.endIndex
            let content = text[range]
            if let open = fence {
                current = current!.lowerBound..<range.upperBound
                if closesFence(content, open.ch, open.len) {
                    fence = nil
                    blocks.append(current!)
                    current = nil
                }
                continue
            }
            if content.allSatisfy(\.isWhitespace) {
                afterBlank = true
                continue
            }
            let item = listItem(content)
            let indented = content.hasPrefix("  ") || content.hasPrefix("\t")
            fence = opensFence(content)
            let isHeading = heading(content)
            let continues = !afterBlank || (list && (item || indented))
            if let block = current, continues, fence == nil, !isHeading {
                current = block.lowerBound..<range.upperBound
                list = list || item
            } else {
                if let block = current { blocks.append(block) }
                current = range
                list = item
            }
            if isHeading, let block = current {
                blocks.append(block)
                current = nil
            }
            afterBlank = false
        }
        if let current { blocks.append(current) }
        return blocks
    }

    /// A fenced code block's language and code; nil for any other markdown. An unclosed fence (still streaming) runs
    /// to the end. A fence indented under a list item takes that indent off each code line, as CommonMark does.
    public static func fence(_ text: String) -> (language: String, code: String)? {
        guard let first = text.split(separator: "\n", omittingEmptySubsequences: false).first(where: { !$0.allSatisfy(\.isWhitespace) }) else { return nil }
        let indent = first.prefix(while: { $0 == " " }).count
        let trimmed = text.trimmingWhitespace()
        guard let marker = ["```", "~~~"].first(where: { trimmed.hasPrefix($0) }) else { return nil }
        let (info, body): (Substring, Substring) = {
            guard let n = trimmed.firstIndex(of: "\n") else { return (trimmed, "") }
            return (trimmed[..<n], trimmed[trimmed.index(after: n)...])
        }()
        let language = info.drop(while: { $0 == marker.first }).split(whereSeparator: \.isWhitespace).first.map(String.init) ?? ""
        let code: Substring
        if let n = body.lastIndex(of: "\n"), body[body.index(after: n)...].trimmingWhitespace().hasPrefix(marker) {
            code = body[..<n]
        } else if body.trimmingWhitespace().hasPrefix(marker) {
            code = ""
        } else {
            code = body
        }
        guard indent > 0 else { return (language, String(code)) }
        let lines = code.split(separator: "\n", omittingEmptySubsequences: false).map { line in
            String(line.dropFirst(min(indent, line.prefix(while: { $0 == " " }).count)))
        }
        return (language, lines.joined(separator: "\n"))
    }

    static func indentRest(_ line: Substring) -> Substring? {
        let rest = line.drop(while: { $0 == " " })
        return line.count - rest.count <= 3 ? rest : nil
    }

    static func opensFence(_ line: Substring) -> (ch: Character, len: Int)? {
        guard let rest = indentRest(line), let ch = rest.first, ch == "`" || ch == "~" else { return nil }
        let len = rest.prefix(while: { $0 == ch }).count
        guard len >= 3, !(ch == "`" && rest.dropFirst(len).contains("`")) else { return nil }
        return (ch, len)
    }

    static func closesFence(_ line: Substring, _ ch: Character, _ len: Int) -> Bool {
        guard let rest = indentRest(line) else { return false }
        let run = rest.prefix(while: { $0 == ch }).count
        return run >= len && rest.dropFirst(run).allSatisfy(\.isWhitespace)
    }

    static func heading(_ line: Substring) -> Bool {
        guard let rest = indentRest(line) else { return false }
        let hashes = rest.prefix(while: { $0 == "#" }).count
        guard (1...6).contains(hashes) else { return false }
        let next = rest.dropFirst(hashes).first
        return next == nil || next == " " || next == "\t"
    }

    static func listItem(_ line: Substring) -> Bool {
        let rest = line.drop(while: \.isWhitespace)
        let digits = rest.prefix(while: \.isASCIIDigit).count
        let marker: Int
        switch digits {
        case 0: marker = rest.first.map { "-*+".contains($0) } == true ? 1 : 0
        case 1...9: marker = rest.dropFirst(digits).first.map { $0 == "." || $0 == ")" } == true ? digits + 1 : 0
        default: marker = 0
        }
        guard marker > 0 else { return false }
        let next = rest.dropFirst(marker).first
        return next == nil || next == " " || next == "\t"
    }
}

extension Character {
    var isASCIIDigit: Bool { asciiValue.map { $0 >= 48 && $0 <= 57 } ?? false }
}

extension StringProtocol {
    func trimmingWhitespace() -> SubSequence {
        let start = firstIndex(where: { !$0.isWhitespace }) ?? endIndex
        let end = lastIndex(where: { !$0.isWhitespace }).map { index(after: $0) } ?? start
        return self[start..<Swift.max(start, end)]
    }
}
