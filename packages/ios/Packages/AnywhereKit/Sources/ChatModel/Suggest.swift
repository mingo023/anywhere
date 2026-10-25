import Wire

/// The `/` and `@` list over the composer, and what it lists from. Offsets into the draft are UTF-8 bytes.
public struct Suggest: Sendable {
    /// The rows the list shows at once.
    public static let shown = 8
    /// How often the servers are asked again while one settles: a sign-in waits on the browser, which tells us nothing
    /// when it ends, or a server still connects.
    public static let settleCheck: Seconds = 3

    /// Each agent's commands as its CLI lists them, by agent id; empty until they arrive.
    public internal(set) var commands: [String: [Command]] = [:]
    /// Each folder's files as pocketd lists them, relative to it; empty while they load.
    public internal(set) var files: [String: [String]] = [:]
    /// Each agent's MCP servers as it last told them, or why it couldn't; an entry of nil while the first answer is on its way.
    public internal(set) var servers: [String: Result<[McpServer], Failure>?] = [:]
    /// The server each agent is acting on, by agent id, until its answer.
    public internal(set) var acting: [String: (server: String, action: ClientMessage.AgentMcp.Action)] = [:]
    /// The servers whose sign-in went to the browser, until they no longer wait on one.
    var signingIn: Set<Pair> = []
    /// The agents whose servers are to be asked again as they settle, until they are.
    var rechecking: Set<String> = []
    public var selected = 0

    public struct Failure: Error, Hashable, Sendable {
        public var message: String
        public init(_ message: String) { self.message = message }
    }

    struct Pair: Hashable, Sendable {
        var agent: String
        var server: String
    }

    public init() {}

    public mutating func listed(_ agent: String, _ commands: [Command]) { self.commands[agent] = commands }

    public mutating func filesListed(_ cwd: String, _ files: [String]) { self.files[cwd] = files }

    /// Takes `agent`'s servers; once `acted` on one, a sign-in URL means it now waits on the browser.
    public mutating func mcpListed(_ agent: String, _ servers: Result<[McpServer], Failure>, authUrl: String, acted: Bool) {
        if acted, let (server, action) = acting.removeValue(forKey: agent), action == .auth, !authUrl.isEmpty {
            signingIn.insert(Pair(agent: agent, server: server))
        }
        if case .success(let list) = servers {
            signingIn = signingIn.filter { p in p.agent != agent || list.contains { $0.name == p.server && $0.status == .needsAuth } }
        }
        self.servers[agent] = .some(servers)
    }

    public mutating func mcpActFailed(_ agent: String) { acting[agent] = nil }

    /// What `agent` is to do to `s` now, as `provider` can; nothing while it acts on another, one at a time.
    public mutating func act(_ agent: String, _ provider: String, _ s: McpServer) -> ClientMessage.AgentMcp.Action? {
        guard acting[agent] == nil, let action = Mcp.action(provider, s) else { return nil }
        acting[agent] = (s.name, action)
        return action
    }

    public func signsIn(_ agent: String, _ server: String) -> Bool {
        signingIn.contains(Pair(agent: agent, server: server))
    }

    /// Whether `agent` waits on a browser sign-in or a server that still connects.
    public func settling(_ agent: String) -> Bool {
        let connecting = listedServers(agent).contains { $0.status == .pending }
        return connecting || signingIn.contains { $0.agent == agent }
    }

    /// Whether to ask `agent`'s servers again shortly: they settle and no ask is due yet.
    public mutating func rechecks(_ agent: String) -> Bool {
        settling(agent) && rechecking.insert(agent).inserted
    }

    public mutating func rechecked(_ agent: String) { rechecking.remove(agent) }

    public func listedServers(_ agent: String) -> [McpServer] {
        if case .success(let list)?? = servers[agent] { return list }
        return []
    }

    /// `text` led by a line asking `agent` to use the MCP servers it tags.
    public func withMcpContext(_ agent: String, _ provider: String, _ text: String) -> String {
        guard case .success(let list)?? = servers[agent] else { return text }
        return Mcp.withContext(text, list, provider)
    }

    /// Whether to ask `agent` for its MCP servers: each time a tag starts, as they connect and drop, and while none came yet.
    public mutating func asksServers(_ agent: String, _ text: String, _ cursor: Int) -> Bool {
        guard let (_, query) = word(text, cursor, "@"), query.hasPrefix(mcpTag.dropFirst()) else { return false }
        let ask = query.count == mcpTag.count - 1 || servers[agent] == nil
        if ask && servers[agent] == nil { servers[agent] = .some(nil) }
        return ask
    }

    /// Whether to ask `agent` for its commands: each time a `/` starts a word, as they can change, and while none came yet.
    public mutating func asksCommands(_ agent: String, _ text: String, _ cursor: Int) -> Bool {
        guard let (_, query) = word(text, cursor, "/") else { return false }
        let ask = query.isEmpty || commands[agent] == nil
        if ask && commands[agent] == nil { commands[agent] = [] }
        return ask
    }

    /// Whether to list `cwd`'s files: each time an `@` starts a mention, and while none came yet.
    public mutating func loadsFiles(_ cwd: String, _ text: String, _ cursor: Int) -> Bool {
        guard let (_, query) = word(text, cursor, "@") else { return false }
        let load = query.isEmpty || files[cwd] == nil
        if load && files[cwd] == nil { files[cwd] = [] }
        return load
    }

    /// Moves the selection one row, wrapping at either end of `total`.
    public mutating func step(down: Bool, total: Int) {
        let at = min(selected, total - 1)
        selected = down ? (at + 1) % total : (at + total - 1) % total
    }
}

/// What the list shows for a draft.
public enum SuggestRows: Sendable {
    /// `at` is where the `/` stands.
    case commands(at: Int, query: String, rows: [Slash])
    /// `at` is where the `@` stands.
    case files(at: Int, query: String, paths: [String])
    /// `at` is where the `@mcp/` tag stands; `servers` hold the query in picker order, nil while they load. The first
    /// `pickable` take a tag or an action.
    case servers(at: Int, query: String, servers: Result<[McpServer], Suggest.Failure>?, pickable: Int)

    /// The rows a pick can take.
    public var count: Int {
        switch self {
        case .commands(_, _, let rows): rows.count
        case .files(_, _, let paths): paths.count
        case .servers(_, _, _, let pickable): pickable
        }
    }
}

extension Suggest {
    /// The list for `text` with the cursor at `cursor` in `agent`'s chat, nil when no word there asks for one.
    public func rows(_ text: String, _ cursor: Int, agent: String, provider: String, cwd: String, touched: @autoclosure () -> [String], changed: Set<String> = []) -> SuggestRows? {
        let adapter = Adapter.for(provider)
        if let (at, query) = word(text, cursor, "/") {
            var rows = menu(adapter, commands[agent] ?? [], query)
            if !startsDraft(text, at) { rows = rows.filter { $0.kind == .send || $0.kind == .local(.mcp) } }
            return .commands(at: at, query: query, rows: rows)
        }
        guard let (at, query) = word(text, cursor, "@") else { return nil }
        if query.hasPrefix(mcpTag.dropFirst()), adapter.command("mcp", "") == .local(.mcp) {
            let query = String(query.dropFirst(mcpTag.count - 1))
            let listed = servers[agent].flatMap { $0 }.map { $0.map { Mcp.picker($0, query) } }
            let all: [McpServer] = if case .success(let l)? = listed { l } else { [] }
            let pickable = all.prefix { Mcp.usable($0) || Mcp.action(provider, $0) != nil }.count
            return .servers(at: at, query: query, servers: listed, pickable: pickable)
        }
        let root = cwd.hasSuffix("/") ? cwd : cwd + "/"
        let near = touched().compactMap { $0.hasPrefix(root) ? String($0.dropFirst(root.count)) : nil }
        return .files(at: at, query: query, paths: rank(query, touched: near, changed: changed, files: files[cwd] ?? []))
    }
}

/// Whether the `/` at `at` starts the draft as `slash` takes it, after whitespace at most.
public func startsDraft(_ text: String, _ at: Int) -> Bool {
    text.utf8.prefix(at).allSatisfy { $0 == 0x20 || $0 == 0x0A || $0 == 0x09 || $0 == 0x0D }
}

/// The draft with command `name` and `arg`, and where the cursor goes: after the name, ready for the argument.
public func named(_ name: String, _ arg: String) -> (text: String, cursor: Int) {
    ("/\(name) \(arg)", name.utf8.count + 2)
}

/// Where the `sigil` word the cursor ends starts, and what follows the `sigil`; it starts the draft or follows whitespace.
public func word(_ text: String, _ cursor: Int, _ sigil: Character) -> (at: Int, query: String)? {
    let bytes = Array(text.utf8)
    guard cursor <= bytes.count else { return nil }
    let before = String(decoding: bytes[..<cursor], as: UTF8.self)
    var start = before.endIndex
    while start > before.startIndex, !before[before.index(before: start)].isWhitespace { start = before.index(before: start) }
    let w = before[start...]
    guard w.first == sigil else { return nil }
    return (before.utf8.distance(from: before.startIndex, to: start), String(w.dropFirst()))
}

/// The `files` that hold the query: those the agent touched, latest first, then those changed, then the rest in order.
public func rank(_ query: String, touched: [String], changed: Set<String>, files: [String]) -> [String] {
    let q = query.lowercased()
    var latest: [String: Int] = [:]
    for (i, t) in touched.enumerated() where latest[t] == nil { latest[t] = i }
    func tier(_ i: Int, _ f: String) -> (Int, Int) {
        if let t = latest[f] { return (0, t) }
        return changed.contains(f) ? (1, i) : (2, i)
    }
    return files.enumerated()
        .filter { q.isEmpty || $0.element.lowercased().contains(q) }
        .map { (tier($0.offset, $0.element), $0.element) }
        .sorted { $0.0 < $1.0 }
        .map(\.1)
}

/// The `Suggest.shown` rows around `selected` of `total`, scrolled only as far as it takes to keep it in view.
public func window(_ selected: Int, _ total: Int) -> Range<Int> {
    let start = min(max(selected - (Suggest.shown - 1), 0), max(total - Suggest.shown, 0))
    return start..<min(total, start + Suggest.shown)
}

/// The draft with `put` in place of the word from `at` to `cursor`, and where the cursor goes after it.
public func replaced(_ text: String, _ at: Int, _ cursor: Int, _ put: String) -> (text: String, cursor: Int) {
    let bytes = Array(text.utf8)
    let head = String(decoding: bytes[..<at], as: UTF8.self) + put
    let rest = String(decoding: bytes[cursor...], as: UTF8.self)
    let gap = rest.first?.isWhitespace == true ? "" : " "
    return (head + gap + rest, head.utf8.count + 1)
}

/// Files the agent read, edited or wrote, its sub-agents' too, latest first, each once.
public func touched(_ t: Transcript) -> [String] {
    func collect(_ t: Transcript, _ paths: inout [String]) {
        for case .tool(let call) in t.turns.lazy.flatMap(\.blocks) {
            switch call.kind {
            case .read(let path), .edit(let path, _), .write(let path, _): paths.append(path)
            case .agent(let sub): collect(sub.transcript, &paths)
            default: break
            }
        }
    }
    var paths: [String] = []
    collect(t, &paths)
    var seen = Set<String>()
    return paths.reversed().filter { seen.insert($0).inserted }
}

/// Of `paths`, those `written` names: itself, or one ending in `/written`.
func naming(_ written: String, _ paths: [String]) -> [String] {
    paths.filter { $0 == written || $0.hasSuffix("/" + written) }
}

/// The files a chip naming `written` may mean: the latest the agent touched, else each in the repo under `cwd`.
public func candidates(_ written: String, cwd: String, touched: [String], repo: () -> [String]) -> [String] {
    if let latest = naming(written, touched).first { return [latest] }
    let root = cwd.hasSuffix("/") ? cwd : cwd + "/"
    return naming(written, repo()).map { root + $0 }
}
