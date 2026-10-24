/// What a prompt's text marks out: the commands and skills it names and the files it mentions, as UTF-8 ranges.
public struct Marks: Hashable, Sendable {
    public var commands: [Range<Int>] = []
    public var mentions: [Range<Int>] = []
}

/// Each `/skill` and `@path` in `text`, at its start or after whitespace. A skill's name starts with a letter and may
/// end in punctuation, as `/review,`, but not run on into a path, as `/usr/bin` or `/notes.md`. An MCP server's tag
/// names no file.
public func marks(_ text: String) -> Marks {
    var marks = Marks()
    var chars: [(Int, Unicode.Scalar)] = []
    var offset = 0
    for u in text.unicodeScalars {
        chars.append((offset, u))
        offset += UTF8.width(u)
    }
    let bytes = Array(text.utf8)
    let tag = Array(mcpTag.utf8)
    var k = 0
    var afterSpace = true
    func take(_ from: Int, _ part: (Unicode.Scalar) -> Bool) -> Int {
        var end = from
        while k < chars.count, part(chars[k].1) {
            end = chars[k].0 + UTF8.width(chars[k].1)
            k += 1
        }
        return end
    }
    while k < chars.count {
        let (i, c) = chars[k]
        k += 1
        if afterSpace && c == "@" {
            let end = take(i + 1) { alphanumeric($0) || "_./-".unicodeScalars.contains($0) }
            if end > i + 1 && !(end - i >= tag.count && Array(bytes[i..<i + tag.count]) == tag) {
                marks.mentions.append(i..<end)
            }
        } else if afterSpace && c == "/" && k < chars.count && chars[k].1.properties.isAlphabetic {
            let name = take(i + 1) { alphanumeric($0) || "_-:".unicodeScalars.contains($0) }
            var end = name
            while end > i + 1 && bytes[end - 1] == UInt8(ascii: ":") { end -= 1 }
            let tail = take(name) { !$0.properties.isWhitespace }
            let after = String(decoding: bytes[end..<tail], as: UTF8.self)
            if after.unicodeScalars.allSatisfy({ !alphanumeric($0) && $0 != "/" }) { marks.commands.append(i..<end) }
        }
        afterSpace = c.properties.isWhitespace
    }
    return marks
}

func alphanumeric(_ u: Unicode.Scalar) -> Bool {
    u.properties.isAlphabetic || u.properties.numericType != nil
}

/// What a mention shows: its file's name, or a folder's name and `/`.
public func mentionName(_ path: String) -> String {
    let trimmed = path.hasSuffix("/") ? path.dropLast() : Substring(path)
    guard let slash = trimmed.lastIndex(of: "/") else { return path }
    return String(path[path.index(after: slash)...])
}
