/// How far a list item's text sits from its marker's edge, where an indented fence is drawn.
public let listIndent = 14 * 1.9

/// A fenced code block's language and code, and how deep it was indented; nil for any other markdown. An unclosed fence
/// (still streaming) runs to the end. A fence indented under a list item takes that indent off each code line, as
/// CommonMark does.
public func fence(_ text: String) -> (language: String, code: String, indent: Int)? {
    guard let first = lines(text).first(where: { !$0.allSatisfy(\.isWhitespace) }) else { return nil }
    let indent = first.prefix { $0 == " " }.count
    let text = text.trimmingWhitespace()
    guard let marker = ["```", "~~~"].first(where: { text.hasPrefix($0) }), let mark = marker.first else { return nil }
    let (info, body): (Substring, Substring) =
        if let nl = text.firstIndex(of: "\n") { (text[..<nl], text[text.index(after: nl)...]) } else { (text[...], "") }
    let language = info.drop { $0 == mark }.split(whereSeparator: \.isWhitespace).first.map(String.init) ?? ""
    let code: Substring
    if let nl = body.lastIndex(of: "\n"), body[body.index(after: nl)...].trimmingWhitespace().hasPrefix(marker) {
        code = body[..<nl]
    } else if body.trimmingWhitespace().hasPrefix(marker) {
        code = ""
    } else {
        code = body
    }
    guard indent > 0 else { return (language, String(code), 0) }
    let dedented: [String] = code.split(separator: "\n", omittingEmptySubsequences: false).map { line in
        String(line.dropFirst(min(line.prefix { $0 == " " }.count, indent)))
    }
    return (language, dedented.joined(separator: "\n"), indent)
}

/// Where a link points.
public enum LinkTarget: Hashable, Sendable {
    case web(String)
    case file(String)
}

/// A URL opens on the web; anything else is a file, under `cwd` unless absolute, without its `:line:col` or `#L` suffix.
public func target(_ link: String, cwd: String) -> LinkTarget {
    if link.contains("://") && !link.hasPrefix("file://") { return .web(link) }
    var path = Substring(link.hasPrefix("file://") ? String(link.dropFirst(7)) : link)
    path = path.split(separator: "#", omittingEmptySubsequences: false).first ?? ""
    for _ in 0..<2 {
        if let colon = path.lastIndex(of: ":") {
            let n = path[path.index(after: colon)...]
            if !n.isEmpty && n.allSatisfy(\.isASCII) && n.allSatisfy(\.isNumber) { path = path[..<colon] }
        }
    }
    if path.hasPrefix("/") { return .file(String(path)) }
    return .file(cwd.hasSuffix("/") ? cwd + path : "\(cwd)/\(path)")
}
