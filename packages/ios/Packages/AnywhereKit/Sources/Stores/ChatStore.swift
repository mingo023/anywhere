import ChatModel
import Client
import Foundation
import Observation
import Wire

/// One open agent's chat: its timeline folded into rows, the outgoing queue and the composer's state.
@MainActor
@Observable
public final class ChatStore: HostStore {
    /// Items asked for when the chat opens and after a reconnect; older ones page in.
    public static let page = 500

    public let agentId: String
    @ObservationIgnored let connection: DaemonConnection?
    @ObservationIgnored let files: (any ChatFiles)?
    @ObservationIgnored let defaults: UserDefaults?
    @ObservationIgnored let clock: @MainActor () -> Int

    public private(set) var summary: AgentSummary?
    public private(set) var session = Session()
    /// The first page came.
    public private(set) var loaded = false
    public private(set) var hasOlder = false
    public private(set) var loadingOlder = false
    /// Open asks of this agent, in the order they came.
    public private(set) var requests: [PermissionRequest] = []
    public private(set) var wizard: Wizard?
    public private(set) var providers: AgentProviders?
    public private(set) var caps: [String] = []
    public private(set) var scopes: [Scope] = []
    public var suggest = Suggest()
    public private(set) var btw: Btw?
    public private(set) var accounts: [Account] = []
    /// Why the last send went nowhere, for the footer.
    public private(set) var notice: String?
    public var draft = Draft() { didSet { persist() } }
    /// Messages being let out as they stream, by message id.
    public private(set) var reveals: [String: Reveal] = [:]
    /// A reply that just settled, by its fold's row id, so its label fades in.
    public private(set) var settled: (row: String, at: Seconds)?
    /// Bumped by each send the chat pins a quarter down; `steers` by each send into the pinned prompt's turn.
    public private(set) var sends = 0
    public private(set) var steers = 0
    public private(set) var mcpOffered = false

    @ObservationIgnored var items: [TimelineItem] = []
    @ObservationIgnored var index: [String: Int] = [:]
    @ObservationIgnored var revision = 0
    /// Items at the front untouched since the last refold.
    @ObservationIgnored var unchanged = 0
    @ObservationIgnored var flushing = false
    @ObservationIgnored var primed = false
    @ObservationIgnored var viewing = false
    @ObservationIgnored var ticker: Task<Void, Never>?
    @ObservationIgnored var requestsMade = 0
    @ObservationIgnored var searching: Task<Void, Never>?

    /// The composer's draft: what was typed and the images attached.
    public struct Draft: Codable, Hashable, Sendable {
        public var text = ""
        public var images: [Attachment] = []
        public init() {}
        public var isEmpty: Bool { text.allSatisfy(\.isWhitespace) && images.isEmpty }
    }

    /// An image on its way to the Mac, or there at `path`.
    public struct Attachment: Codable, Hashable, Sendable, Identifiable {
        public var id: String
        public var name: String
        public var path: String?
        public var failed = false
        /// A small rendition, for the tile.
        public var thumbnail: Data?
    }

    /// A `/btw` side question and how it stands.
    public struct Btw: Hashable, Sendable {
        public var question: String
        public var answer: String?
        public var error: String?
    }

    public init(
        agentId: String, connection: DaemonConnection? = nil, files: (any ChatFiles)? = nil, defaults: UserDefaults? = nil,
        draftScope: String = "", clock: @escaping @MainActor () -> Int = { Int(Date().timeIntervalSince1970 * 1000) }
    ) {
        self.agentId = agentId
        draftKey = "chat.draft.\(draftScope).\(agentId)"
        self.connection = connection
        self.files = files
        self.defaults = defaults
        self.clock = clock
        if let data = defaults?.data(forKey: draftKey), let saved = try? JSONDecoder().decode(Draft.self, from: data) { draft = saved }
    }

    @ObservationIgnored let draftKey: String

    // MARK: Messages

    public func apply(_ message: ServerMessage) -> Bool {
        switch message {
        case .agentList(let list):
            guard let agent = list.agents.first(where: { $0.id == agentId }) else { return false }
            return take(agent)
        case .agentUpdate(let update):
            guard update.agent.id == agentId else { return false }
            return take(update.agent)
        case .agentStream(let stream):
            guard stream.agentId == agentId else { return false }
            upsert([stream.item])
            return true
        case .agentTimeline(let timeline):
            guard timeline.agentId == agentId else { return false }
            page(timeline, older: false)
            return true
        case .permissionRequest(let ask):
            guard ask.request.agentId == agentId else { return false }
            if let at = requests.firstIndex(where: { $0.requestId == ask.request.requestId }) {
                requests[at] = ask.request
            } else {
                requests.append(ask.request)
            }
            refreshWizard()
            return true
        case .permissionResolved(let resolved):
            guard let at = requests.firstIndex(where: { $0.requestId == resolved.requestId }) else { return false }
            requests.remove(at: at)
            refreshWizard()
            return true
        case .agentCommands(let commands):
            guard commands.agentId == agentId else { return false }
            suggest.listed(agentId, commands.commands)
            return true
        case .agentMcp(let mcp):
            guard mcp.agentId == agentId else { return false }
            suggest.mcpListed(agentId, .success(mcp.servers), authUrl: mcp.authUrl ?? "", acted: false)
            return true
        case .agentBtw(let answer):
            guard answer.agentId == agentId, btw != nil else { return false }
            btw?.answer = answer.answer
            return true
        case .agentProviders(let p):
            providers = p
            return true
        default:
            return false
        }
    }

    /// What the Mac already told about the agent before its chat opened.
    public func seed(_ agent: AgentSummary?, requests: [PermissionRequest], hello: HelloOk?) {
        if let agent { _ = take(agent) }
        self.requests = requests
        caps = hello?.caps ?? []
        scopes = hello?.scopes ?? []
        refreshWizard()
    }

    public func connected(_ hello: HelloOk) {
        caps = hello.caps ?? []
        scopes = hello.scopes ?? []
        requests = []
        refreshWizard()
        if viewing { open() }
    }

    public func disconnected() {
        loadingOlder = false
    }

    /// Takes a newer summary; a timeline pocketd started over drops what was held.
    func take(_ agent: AgentSummary) -> Bool {
        guard agent != summary else { return false }
        let reset = summary.map { $0.resets != agent.resets } ?? false
        let wasWorking = summary.map(Composer.working)
        summary = agent
        if reset {
            items = []
            index = [:]
            unchanged = 0
            revision += 1
            primed = false
            loaded = false
            hasOlder = false
            fetch()
        }
        if reset || wasWorking != Composer.working(agent) { scheduleFlush() }
        return true
    }

    /// Takes a page of the timeline: the latest, or with `older`, the one before what's held.
    func page(_ timeline: AgentTimeline, older: Bool) {
        if older || !loaded { hasOlder = timeline.hasOlder }
        loaded = true
        upsert(timeline.items)
    }

    /// Takes items in as the desktop does: replacing the one with its id, then in seq order.
    func upsert(_ new: [TimelineItem]) {
        guard !new.isEmpty else { return }
        var sorted = true
        for item in new {
            if let at = index[item.id] {
                if items[at] == item { continue }
                if items[at].seq != item.seq { sorted = false }
                items[at] = item
                unchanged = min(unchanged, at)
            } else {
                if let last = items.last, last.seq > item.seq { sorted = false }
                index[item.id] = items.count
                unchanged = min(unchanged, items.count)
                items.append(item)
            }
        }
        if !sorted {
            let ids = items.map(\.id)
            items.sort { $0.seq < $1.seq }
            let first = items.indices.first { ids[$0] != items[$0].id } ?? items.count
            unchanged = min(unchanged, first)
            for (i, item) in items.enumerated() { index[item.id] = i }
        }
        revision += 1
        scheduleFlush()
    }

    /// Refolds once per burst of messages, after the batch that brought them.
    func scheduleFlush() {
        guard !flushing else { return }
        flushing = true
        Task { @MainActor [weak self] in self?.flush() }
    }

    /// Brings the rows up to the items and pocketd's status, lets out what grew, and sends the queued prompts it releases.
    public func flush() {
        flushing = false
        let before = textLengths(session.transcript)
        let tail = session.liveTail.map { session.transcript.turns[$0].id }
        let working = summary.map(Composer.working) ?? false
        let hint = min(unchanged, items.count)
        unchanged = items.count
        if session.update(revision, items, working: working, now: clock(), unchanged: hint) != nil {
            let now = Date.timeIntervalSinceReferenceDate
            if let turn = settledTail(tail, session.transcript) { settled = ("\(turn)#fold", now) }
            if primed {
                streamIn(&reveals, before: before, session.transcript, now: now)
            } else {
                primed = !session.transcript.turns.isEmpty
            }
        }
        for (id, text) in session.outgoing.released() {
            let key = prompt(text)
            session.outgoing.sentAs(id, key)
        }
        refreshWizard()
    }

    // MARK: On screen

    /// The chat came on screen: fetch what's missing, tell pocketd it's viewed, keep the queue moving.
    public func appear() {
        viewing = true
        open()
        if let cwd = summary?.cwd, let path = files?.takeMention(in: cwd) { mention(path) }
        ticker?.cancel()
        ticker = Task { @MainActor [weak self] in
            while !Task.isCancelled {
                try? await Task.sleep(for: .seconds(1))
                self?.flush()
            }
        }
    }

    /// Puts `@path` at the end of the draft, as a pick from the suggest list would.
    func mention(_ path: String) {
        let text = draft.text
        draft.text = text + (text.isEmpty || text.last!.isWhitespace ? "" : " ") + "@" + path + " "
    }

    public func disappear() {
        viewing = false
        ticker?.cancel()
        ticker = nil
        send(.agentView(.init(agentIds: [])))
    }

    func open() {
        send(.agentView(.init(agentIds: [agentId])))
        fetch()
        if providers == nil { request(.agentProviders) { [weak self] in _ = self?.apply($0) } }
        if let agent = summary, agent.chatOnly == true {
            request(.accountList(.init(provider: agent.provider))) { [weak self] reply in
                if case .accountList(let list) = reply { self?.accounts = list.accounts }
            }
        }
    }

    /// Asks for the latest page; it upserts over what's held, so a reconnect keeps the pages paged in.
    func fetch() {
        request(.agentTimeline(.init(agentId: agentId, limit: Self.page))) { [weak self] reply in
            if case .agentTimeline(let t) = reply { self?.page(t, older: false) }
        }
    }

    /// Pages in the items before the oldest held.
    public func loadOlder() {
        guard hasOlder, !loadingOlder, let oldest = items.first?.seq else { return }
        loadingOlder = true
        request(.agentTimeline(.init(agentId: agentId, beforeSeq: oldest, limit: Self.page))) { [weak self] reply in
            guard let self else { return }
            loadingOlder = false
            if case .agentTimeline(let t) = reply { page(t, older: true) }
        } failed: { [weak self] _ in
            self?.loadingOlder = false
        }
    }

    // MARK: State the composer reads

    public var provider: String { summary?.provider ?? "" }
    public var adapter: Adapter { Adapter.for(provider) }

    public var capabilities: Capabilities {
        ChatModel.capabilities(provider, providers?.providers.first { $0.id == provider }?.chat)
    }

    public var observeOnly: Bool { !scopes.isEmpty && !scopes.contains(.drive) && !scopes.contains(.owner) }

    /// `permission.resolve` needs the approve scope, which a phone may hold without drive.
    public var canApprove: Bool { scopes.isEmpty || scopes.contains(.approve) || scopes.contains(.owner) }

    /// The ask the agent waits on, as the chat shows it.
    public var ask: Ask? { requests.first.map(ChatModel.ask) }

    public var feedback: Bool { requests.first?.feedback ?? false }

    public var mode: Composer.Mode { Composer.mode(observeOnly: observeOnly, canApprove: canApprove, ask: ask) }

    public var busy: Bool { summary.map { Composer.busy($0, session) } ?? false }

    public var stopping: Bool { session.outgoing.stopping }

    public var action: Action { ChatModel.action(working: busy, stopping: stopping, empty: draft.isEmpty) }

    public var uploading: Bool { draft.images.contains { $0.path == nil && !$0.failed } }

    public func hasCap(_ cap: String) -> Bool { caps.contains(cap) }

    // MARK: Sending

    /// Sends the draft unless the agent can't take it now. While the agent works a send goes into the running turn; behind
    /// a stop, or to `queue`, it waits for the turn to end.
    public func send(queue: Bool = false) {
        guard mode == .prompt, !uploading, let agent = summary else { return }
        let busy = busy
        if busy && !capabilities.sendWhileWorking { return }
        let steer = busy && !stopping && !queue
        let typed = draft.text
        let text = ChatModel.prompt(typed, images: draft.images.compactMap(\.path))
        guard !text.isEmpty else { return }
        let command = slash(typed).map { (adapter.command($0.name, $0.arg), $0.name, $0.arg) }
        if case (.unsupported, let name, _)? = command {
            notice = Composer.terminalOnlySaid("/\(name)", provider: provider, chatOnly: agent.chatOnly ?? false)
            return
        }
        notice = nil
        draft = Draft()
        if case (.local(let local), _, let arg)? = command { return runLocal(local, arg) }
        let sent = command != nil ? text : suggest.withMcpContext(agentId, provider, text)
        if skipsQueue(typed) {
            _ = prompt(sent)
            return
        }
        if !steer { return push(sent, busy: busy) }
        let id = session.outgoing.steerText(sent, now: clock())
        session.outgoing.sentAs(id, prompt(sent))
        steers += 1
        scheduleFlush()
    }

    /// Sends `text` now if the agent is idle, else queues it for after the turn; a send now is pinned a quarter down.
    func push(_ text: String, busy: Bool) {
        if let id = session.outgoing.push(text, working: busy, now: clock()), !busy {
            session.outgoing.sentAs(id, prompt(text))
        }
        if !busy { sends += 1 }
        scheduleFlush()
    }

    /// Runs a command the phone does itself.
    func runLocal(_ local: ChatModel.Local, _ arg: String) {
        switch (local, arg) {
        case (.model, ""), (.effort, ""): settingsMenu = local
        case (.new, _), (.btw, ""): break
        case (.model, let model): settings(model: model)
        case (.effort, let effort): settings(effort: effort)
        case (.compact, _): compact()
        case (.btw, let question): ask(btw: question)
        case (.mcp, let query): draft.text = mcpTag + query
        }
    }

    /// Sends `text` as an `agent.prompt` and returns the key a refusal names it by.
    @discardableResult
    func prompt(_ text: String) -> String {
        requestsMade += 1
        let key = "p\(requestsMade)"
        request(.agentPrompt(.init(agentId: agentId, text: text))) { _ in } failed: { [weak self] _ in
            self?.session.outgoing.fail(key)
            self?.scheduleFlush()
        }
        return key
    }

    public func stop() {
        session.outgoing.stop(now: clock())
        request(.agentInterrupt(.init(agentId: agentId))) { _ in } failed: { [weak self] _ in
            self?.session.outgoing.stopRefused()
        }
        scheduleFlush()
    }

    /// Stops the sub-agents a chat agent left in the background; no turn runs, so nothing waits on a stop.
    public func stopBackground() { request(.agentInterrupt(.init(agentId: agentId))) { _ in } }

    /// What the send button does when pressed: stops a working agent with an empty draft, else sends.
    public func press() {
        if action == .stop { stop() } else { send() }
    }

    public func sendNow() {
        guard session.outgoing.sendNow(now: clock()) else { return }
        request(.agentSendNow(.init(agentId: agentId))) { _ in }
        scheduleFlush()
    }

    public func compact() { request(.agentCompact(.init(agentId: agentId))) { _ in } }

    /// Sends a queued send into the turn now.
    public func steerQueued(_ id: Int) {
        guard let text = session.outgoing.steer(id, now: clock()) else { return }
        session.outgoing.sentAs(id, prompt(text))
        steers += 1
        scheduleFlush()
    }

    /// Pulls a queued send back into the composer, under what's typed.
    public func editQueued(_ id: Int) {
        guard let text = session.outgoing.withdraw(id) else { return }
        draft.text = Queue.editing(draft.text, text)
        scheduleFlush()
    }

    public func removeQueued(_ id: Int) {
        _ = session.outgoing.withdraw(id)
        scheduleFlush()
    }

    /// Puts a send that wasn't delivered back in the composer.
    public func retry(_ id: Int) {
        guard let text = session.outgoing.retry(id) else { return }
        draft.text = Queue.editing(draft.text, splitImages(text).text)
        scheduleFlush()
    }

    public func resume() {
        session.outgoing.resume(session.transcript)
        scheduleFlush()
    }

    /// Re-sends the prompt that set a reply going, leaving the draft alone.
    public func retryTurn(_ turn: Int) {
        guard mode == .prompt, summary != nil, session.transcript.turns.indices.contains(turn),
            let text = session.transcript.turns[turn].prompt?.text
        else { return }
        let busy = busy
        if busy && !capabilities.sendWhileWorking { return }
        push(text, busy: busy)
    }

    // MARK: Asks

    public func allow() { resolve(.allow) }

    public func deny(_ message: String? = nil) {
        let message = message?.trimmingCharacters(in: .whitespacesAndNewlines)
        resolve(.deny, message: message?.isEmpty == false ? message : nil)
    }

    func resolve(_ decision: Decision, message: String? = nil, answers: [String: String]? = nil) {
        guard let request = requests.first else { return }
        let id = request.requestId
        self.request(.permissionResolve(.init(requestId: id, decision: decision, message: message, answers: answers))) { _ in } failed: {
            [weak self] error in
            guard let self else { return }
            notice = error
            // The ask still stands, so its questions can be sent again.
            if wizard?.requestId == id { wizard?.sent = false }
        }
    }

    /// The question wizard's step: a pick, the next page, back or skip.
    public func wizard(_ change: (inout Wizard) -> Wizard.Step) {
        guard var w = wizard, !w.sent else { return }
        let step = change(&w)
        wizard = w
        if case .done(let answers) = step {
            wizard?.sent = true
            resolve(.allow, answers: answers)
        }
    }

    /// Skips the questions: a deny that stops the agent's turn.
    public func skipQuestions() {
        guard wizard?.sent == false else { return }
        wizard?.sent = true
        resolve(.deny)
    }

    func refreshWizard() {
        guard let ask, !ask.questions.isEmpty else {
            if wizard != nil { wizard = nil }
            return
        }
        if wizard?.requestId != ask.requestId { wizard = Wizard(ask) }
    }

    // MARK: Settings, side questions, servers

    public func settings(model: String? = nil, effort: String? = nil, access: String? = nil) {
        request(.agentSettings(.init(agentId: agentId, model: model, effort: effort, access: access))) { _ in }
    }

    public func pin(_ pinned: Bool) {
        request(.agentPin(.init(agentId: agentId, pinned: pinned))) { _ in }
    }

    public func switchAccount(_ account: String) {
        request(.agentAccount(.init(agentId: agentId, account: account))) { _ in }
    }

    public func ask(btw question: String) {
        btw = Btw(question: question)
        request(.agentBtw(.init(agentId: agentId, text: question))) { [weak self] reply in
            if case .agentBtw(let a) = reply { self?.btw?.answer = a.answer }
        } failed: { [weak self] error in
            self?.btw?.error = error
        }
    }

    public func closeBtw() { btw = nil }

    /// Pauses (Codex) or clears the goal at once, ahead of any queue.
    public func goal(_ verb: String) { _ = prompt("/goal \(verb)") }

    /// Lists the agent's slash commands, as each `/` that starts a word asks.
    public func listCommands() {
        request(.agentCommands(.init(agentId: agentId))) { [weak self] in _ = self?.apply($0) }
    }

    /// Lists the agent's MCP servers, or acts on one of them first.
    public func listServers(acting action: ClientMessage.AgentMcp.Action? = nil, on server: String? = nil) {
        let agent = agentId
        request(.agentMcp(.init(agentId: agentId, action: action, server: server))) { [weak self] reply in
            guard let self, case .agentMcp(let m) = reply else { return }
            suggest.mcpListed(agent, .success(m.servers), authUrl: m.authUrl ?? "", acted: action != nil)
            if let url = m.authUrl, !url.isEmpty, action == .auth { authUrl = url }
            recheckServers()
        } failed: { [weak self] error in
            guard let self else { return }
            if action != nil { suggest.mcpActFailed(agent) } else { suggest.mcpListed(agent, .failure(.init(error)), authUrl: "", acted: false) }
        }
    }

    /// The model or effort list `/model` or `/effort` asked to open.
    public var settingsMenu: ChatModel.Local?

    /// A sign-in page an MCP server asked the user to open.
    public var authUrl: String?

    public func openedAuthUrl() { authUrl = nil }

    /// Picks `server` off the list: a usable one is tagged, one that isn't is signed in to, reconnected or enabled.
    public func act(on server: McpServer) {
        guard let action = suggest.act(agentId, provider, server) else { return }
        listServers(acting: action, on: server.name)
    }

    func recheckServers() {
        guard suggest.rechecks(agentId) else { return }
        Task { @MainActor [weak self] in
            try? await Task.sleep(for: .seconds(Suggest.settleCheck))
            guard let self else { return }
            suggest.rechecked(agentId)
            if suggest.settling(agentId) { listServers() }
        }
    }

    /// The draft changed with the cursor at `cursor` (UTF-8 bytes): asks for what its `/` or `@` word lists.
    public func drafted(cursor: Int) {
        let text = draft.text
        if suggest.asksCommands(agentId, text, cursor) { listCommands() }
        if suggest.asksServers(agentId, text, cursor) { listServers() }
        if let (_, query) = word(text, cursor, "@"), !query.hasPrefix(mcpTag.dropFirst()), let cwd = summary?.cwd, let files {
            searching?.cancel()
            searching = Task { @MainActor [weak self] in
                try? await Task.sleep(for: .milliseconds(120))
                guard !Task.isCancelled, let found = try? await files.search(tree: cwd, query: query), !Task.isCancelled else { return }
                self?.suggest.filesListed(cwd, found)
            }
        }
    }

    /// The `/` or `@` list for the draft with the cursor at `cursor`.
    public func suggestions(cursor: Int) -> SuggestRows? {
        guard let agent = summary else { return nil }
        return suggest.rows(draft.text, cursor, agent: agentId, provider: agent.provider, cwd: agent.cwd, touched: touched(session.transcript))
    }

    /// Picks row `ix` of that list: a command or a path goes in for the word typed, a command the phone runs itself
    /// runs, and a server the agent can't use yet is signed in to, reconnected or enabled.
    public func pick(_ ix: Int, cursor: Int) {
        guard let rows = suggestions(cursor: cursor) else { return }
        let text = draft.text
        let utf8 = Array(text.utf8)
        func around(_ at: Int, _ put: String) -> String {
            String(decoding: utf8[..<at], as: UTF8.self) + put + String(decoding: utf8[min(cursor, utf8.count)...], as: UTF8.self)
        }
        suggest.selected = 0
        switch rows {
        case .servers(let at, _, .success(let all)?, let pickable) where ix < pickable:
            if !Mcp.usable(all[ix]) { return act(on: all[ix]) }
            draft.text = replaced(text, at, cursor, mcpTag + all[ix].name).text
        case .commands(let at, _, let list) where list.indices.contains(ix) && !startsDraft(text, at):
            draft.text = list[ix].kind == .local(.mcp) ? around(at, mcpTag) : replaced(text, at, cursor, "/\(list[ix].name)").text
        case .commands(_, _, let list) where list.indices.contains(ix):
            let arg = slash(text)?.arg ?? ""
            if case .local(let local) = adapter.command(list[ix].name, arg) {
                draft.text = ""
                runLocal(local, arg)
            } else {
                draft.text = named(list[ix].name, arg).text
            }
        case .files(let at, _, let paths) where paths.indices.contains(ix):
            draft.text = replaced(text, at, cursor, "@" + paths[ix]).text
        default:
            break
        }
    }

    // MARK: Images

    /// Attaches an image and uploads it; the send waits until it lands.
    public func attach(_ data: Data, name: String, thumbnail: Data? = nil) {
        let id = UUID().uuidString
        draft.images.append(Attachment(id: id, name: name, thumbnail: thumbnail))
        guard let files else { return mark(id) { $0.failed = true } }
        Task { @MainActor [weak self] in
            do {
                let path = try await files.upload(data, name: name)
                self?.mark(id) { $0.path = path }
            } catch {
                self?.mark(id) { $0.failed = true }
            }
        }
    }

    public func detach(_ id: String) { draft.images.removeAll { $0.id == id } }

    func mark(_ id: String, _ change: (inout Attachment) -> Void) {
        guard let at = draft.images.firstIndex(where: { $0.id == id }) else { return }
        change(&draft.images[at])
    }

    // MARK: Rows

    public func toggle(_ id: String, _ fallback: Bool) { session.toggle(id, fallback) }

    /// Opens what hides `row`: the shut fold over it and its shut phase.
    public func show(_ row: Int) {
        if let fold = session.shutFoldOver(row) { session.toggle(fold, false) }
        if session.shutPhase(row), session.rows.indices.contains(row) { session.toggle(session.ids[row], false) }
    }

    /// The transcript a sub-agent's call holds, for its own view.
    public func subAgent(_ callId: String) -> SubAgent? {
        guard let (turn, block) = findCall(session.transcript, callId) else { return nil }
        return session.transcript.turns[turn].blocks[block].call?.agent
    }

    // MARK: Plumbing

    func persist() {
        guard let defaults else { return }
        var kept = draft
        // Thumbnails are whole photos; encoding them on each keystroke would stall typing.
        kept.images = kept.images.filter { $0.path != nil }.map { var a = $0; a.thumbnail = nil; return a }
        if kept.isEmpty {
            defaults.removeObject(forKey: draftKey)
        } else if let data = try? JSONEncoder().encode(kept) {
            defaults.set(data, forKey: draftKey)
        }
    }

    func send(_ message: ClientMessage) {
        guard let connection else { return }
        Task { _ = try? await connection.send(message) }
    }

    /// Sends a request; `done` takes its reply, `failed` why it failed, both on the main actor.
    func request(
        _ message: ClientMessage, done: @escaping @MainActor (ServerMessage) -> Void,
        failed: @escaping @MainActor (String) -> Void = { _ in }
    ) {
        // Even offline the answer comes later, as a caller keys a send by what this returns before it fails.
        guard let connection else {
            Task { @MainActor in failed("Not connected") }
            return
        }
        Task { @MainActor in
            do {
                done(try await connection.request(message))
            } catch DaemonError.server(let error) {
                failed(error.message)
            } catch {
                failed("\(error)")
            }
        }
    }
}
