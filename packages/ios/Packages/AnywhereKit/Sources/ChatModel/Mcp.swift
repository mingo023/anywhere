import Wire

/// What a picked server is written as in the draft, before its name.
public let mcpTag = "@mcp/"

public enum Mcp {
    /// Whether the agent can use `s` now: it is connected, or still connecting.
    public static func usable(_ s: McpServer) -> Bool {
        s.status == .connected || s.status == .pending
    }

    /// What picking `s` does when the agent can't use it now: starts its sign-in, afresh when one started already, as its
    /// browser tab may be gone; reconnects a failed one; enables a disabled one where `provider` can.
    public static func action(_ provider: String, _ s: McpServer) -> ClientMessage.AgentMcp.Action? {
        switch s.status {
        case .needsAuth: .auth
        case .failed: .reconnect
        case .disabled where provider == "claude": .enable
        default: nil
        }
    }

    /// The `servers` whose name or scope holds `query`, each by name: the usable first, then those waiting on a sign-in,
    /// the failed and the disabled. So what a pick can take, a tag or an action, leads.
    public static func picker(_ servers: [McpServer], _ query: String) -> [McpServer] {
        let query = query.lowercased()
        func rank(_ s: McpServer) -> Int {
            if usable(s) { return 0 }
            switch s.status {
            case .needsAuth: return 1
            case .failed: return 2
            default: return 3
            }
        }
        let rows = servers.filter { query.isEmpty || "\($0.name) \($0.scope ?? "")".lowercased().contains(query) }
        return rows.sorted { (rank($0), $0.name.lowercased()) < (rank($1), $1.name.lowercased()) }
    }

    /// The `servers` `text` tags, in the order it does. A tag starts a word and ends at a space or punctuation; where two
    /// names start alike, the longer one that fits is the tag.
    public static func tagged(_ text: String, _ servers: [McpServer]) -> [McpServer] {
        let bytes = Array(text.utf8)
        var at: [Int: McpServer] = [:]
        for s in servers {
            let tag = Array("\(mcpTag)\(s.name)".utf8)
            var i = 0
            while i + tag.count <= bytes.count {
                guard bytes[i] == tag[0], Array(bytes[i..<i + tag.count]) == tag else {
                    i += 1
                    continue
                }
                let before = String(decoding: bytes[..<i], as: UTF8.self).unicodeScalars.last
                let after = String(decoding: bytes[(i + tag.count)...], as: UTF8.self).unicodeScalars.first
                let starts = before.map { $0.properties.isWhitespace || "([{".unicodeScalars.contains($0) } ?? true
                let ends = after.map { $0.properties.isWhitespace || ".,;:!?)]}".unicodeScalars.contains($0) } ?? true
                if starts && ends && (at[i].map { $0.name.utf8.count < s.name.utf8.count } ?? true) { at[i] = s }
                i += tag.count
            }
        }
        var out: [McpServer] = []
        for key in at.keys.sorted() where !out.contains(where: { $0.name == at[key]!.name }) {
            out.append(at[key]!)
        }
        return out
    }

    /// `text` as the agent gets it: led by a line asking it to use the servers it tags, as `provider` has them.
    public static func withContext(_ text: String, _ servers: [McpServer], _ provider: String) -> String {
        let tagged = tagged(text, servers)
        if tagged.isEmpty { return text }
        let names = tagged.map { "\(debugQuoted($0.name)) (\(provider))" }
        let plural = tagged.count == 1 ? "" : "s"
        return "MCP context: Use the configured server\(plural) \(names.joined(separator: ", ")) when relevant to this request.\n\n\(text)"
    }

    /// A string as Rust's `{:?}` writes it.
    static func debugQuoted(_ s: String) -> String {
        var out = "\""
        for c in s.unicodeScalars {
            switch c {
            case "\"": out += "\\\""
            case "\\": out += "\\\\"
            case "\n": out += "\\n"
            case "\r": out += "\\r"
            case "\t": out += "\\t"
            default: out.unicodeScalars.append(c)
            }
        }
        return out + "\""
    }
}
