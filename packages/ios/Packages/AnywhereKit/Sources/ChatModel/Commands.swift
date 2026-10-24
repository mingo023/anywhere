import Wire

/// What the chat does with a slash command.
public enum Kind: Hashable, Sendable {
    /// Sent to the agent as a prompt.
    case send
    /// Run by the phone itself.
    case local(Local)
    /// Needs the agent's terminal, which the chat can't show.
    case unsupported
}

public enum Local: Hashable, Sendable {
    case model, effort, compact, new
    /// A side question, answered beside the conversation.
    case btw
    /// Picks an MCP server for the draft to tag.
    case mcp
}

/// A command an agent offers before its CLI lists any.
public struct Builtin: Hashable, Sendable {
    public let name: String
    public let hint: String
    public let description: String
}

/// A row of the `/` menu.
public struct Slash: Hashable, Sendable {
    public var name: String
    public var hint: String
    public var description: String
    public var kind: Kind

    public init(name: String, hint: String, description: String, kind: Kind) {
        self.name = name
        self.hint = hint
        self.description = description
        self.kind = kind
    }
}

/// The command `text` runs and its argument; nil when it is a prompt, as one that starts with a path.
public func slash(_ text: String) -> (name: String, arg: String)? {
    let trimmed = text.drop(while: \.isWhitespace)
    guard trimmed.first == "/" else { return nil }
    let rest = trimmed.dropFirst()
    let name: Substring
    let arg: Substring
    if let space = rest.firstIndex(where: \.isWhitespace) {
        name = rest[..<space]
        arg = rest[rest.index(after: space)...]
    } else {
        (name, arg) = (rest, "")
    }
    guard !name.isEmpty, !name.contains("/"), !name.contains(".") else { return nil }
    return (String(name), arg.trimmingWhitespace())
}

/// Whether `text` goes at once rather than into the queue: a goal can keep the turn it would wait out going for long.
public func skipsQueue(_ text: String) -> Bool {
    guard let s = slash(text) else { return false }
    return s.name == "goal" && s.arg == "clear"
}

/// The commands whose name holds `query`, those it starts first: the adapter's own, then what its CLI `listed`, without
/// what the chat can't run.
public func menu(_ adapter: Adapter, _ listed: [Command], _ query: String) -> [Slash] {
    let query = query.lowercased()
    let own = adapter.builtins.map { ($0.name, $0.hint, $0.description) }
    let theirs = listed.map { ($0.name, $0.argumentHint ?? "", $0.description ?? "") }
    var rows: [Slash] = []
    for (name, hint, description) in own + theirs {
        if !(query.isEmpty || name.lowercased().contains(query)) || rows.contains(where: { $0.name == name }) { continue }
        let kind = adapter.command(name, "")
        if kind != .unsupported { rows.append(Slash(name: name, hint: hint, description: description, kind: kind)) }
    }
    let starting = rows.filter { $0.name.lowercased().hasPrefix(query) }
    return starting + rows.filter { !$0.name.lowercased().hasPrefix(query) }
}

extension StringProtocol {
    func trimmingWhitespace() -> String {
        guard let first = firstIndex(where: { !$0.isWhitespace }), let last = lastIndex(where: { !$0.isWhitespace }) else { return "" }
        return String(self[first...last])
    }
}
