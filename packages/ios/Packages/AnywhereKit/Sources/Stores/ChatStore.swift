import ChatModel
import Client
import Domain
import Foundation
import Observation
import Wire

/// One open agent's chat: its timeline folded into rows, the outgoing queue and the composer's state.
@MainActor
@Observable
public final class ChatStore: HostStore {
    /// Items asked for when the chat opens and after a reconnect; older ones page in.
    public static let page = 500
    /// The largest thumbnail a saved draft keeps.
    static let thumbnailMax = 64 * 1024

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
    /// The Mac's link is up. While it isn't, sends, stops and answers wait and the draft stays.
    public private(set) var online = true
    /// The session ended, or the Mac's list no longer has it.
    public private(set) var closed = false
    /// Send was pressed while images upload: the draft goes once they land.
    public private(set) var armed = false
    /// The ask an answer is on its way to, until pocketd says it's resolved or the answer fails.
    public private(set) var answering: String?
    /// A New Session `/new` asked to open, until the screen opens it.
    public private(set) var newSession: NewSession?
    /// The main agent's call a sub-agent's screen asked the chat to show, until the chat shows it.
    public private(set) var revealing: String?
    /// The New Session the screen opened, with the draft that asked for it, until a session like it starts.
    @ObservationIgnored var asked: (session: NewSession, draft: Draft)?
    /// Messages being let out as they stream, by message id.
    public private(set) var reveals: [String: Reveal] = [:]
    /// A reply that just settled, by its fold's row id, so its label fades in.
    public private(set) var settled: (row: String, at: Seconds)?
    /// Bumped by each send the chat pins a quarter down; `steers` by each send into the pinned prompt's turn.
    public private(set) var sends = 0
    public private(set) var steers = 0
    public private(set) var mcpOffered = false
    /// Sub-agents and commands at work after their turn ended, and the sub-agents the map shows: worked out once per
    /// refold, so the composer doesn't walk the transcript on each keystroke.
    public private(set) var background: [Call] = []
    public private(set) var agents: [Spawned] = []

    @ObservationIgnored var items: [TimelineItem] = []
    @ObservationIgnored var index: [String: Int] = [:]
    @ObservationIgnored var revision = 0
    /// Items at the front untouched since the last refold.
    @ObservationIgnored var unchanged = 0
    @ObservationIgnored var flushing = false
    @ObservationIgnored var primed = false
    /// Screens showing the chat or one of its sub-agents: one opening over another may appear before the other goes.
    @ObservationIgnored var viewers = 0
    public var viewing: Bool { viewers > 0 }
    @ObservationIgnored var ticker: Task<Void, Never>?
    @ObservationIgnored var requestsMade = 0
    @ObservationIgnored var searching: Task<Void, Never>?
    @ObservationIgnored var macName = ErrorText.unnamedMac
    @ObservationIgnored var armedQueue = false
    /// When the last send went, which Stop ignores for a moment after.
    @ObservationIgnored var sentAt: Int?
    /// Each image's data until it lands, so a failed upload can go again.
    @ObservationIgnored var uploadData: [String: Data] = [:]
    /// Thumbnails of landed images by path, for a send that comes back to the composer.
    @ObservationIgnored var thumbnails: [String: Data] = [:]

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
        /// Why its upload failed.
        public var reason: String?
        /// A small rendition, for the tile.
        public var thumbnail: Data?

        public init(id: String, name: String, path: String? = nil, thumbnail: Data? = nil) {
            self.id = id
            self.name = name
            self.path = path
            self.thumbnail = thumbnail
        }
    }

    /// A New Session preset to the folder this chat's agent runs in, with what followed `/new` as its prompt.
    public struct NewSession: Hashable, Sendable {
        public var cwd: String?
        public var prompt: String?
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
            guard let agent = list.agents.first(where: { $0.id == agentId }) else {
                guard !closed else { return false }
                closed = true
                return true
            }
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
            if answering == resolved.requestId { answering = nil }
            notice = nil
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
        if let hello { macName = hello.hostname }
        refreshWizard()
    }

    /// Whether the Mac named `mac` is online, as the screen sees its host before this store hears of a change.
    public func follow(online: Bool, mac: String) {
        macName = mac
        if online != self.online { self.online = online }
        if online, notice != nil { notice = nil }
    }

    public func connected(_ hello: HelloOk) {
        caps = hello.caps ?? []
        scopes = hello.scopes ?? []
        macName = hello.hostname
        online = true
        notice = nil
        requests = []
        answering = nil
        refreshWizard()
        // Not agent.view: the host sends it, as in the background it must say nothing is viewed.
        if viewing { open() }
    }

    public func disconnected() {
        loadingOlder = false
        online = false
    }

    /// Takes a newer summary; a timeline pocketd started over drops what was held.
    func take(_ agent: AgentSummary) -> Bool {
        let isClosed = agent.status == .closed
        let flipped = closed != isClosed
        if flipped { closed = isClosed }
        guard agent != summary else { return flipped }
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

    /// Takes a page of the timeline: the latest, or with `older`, the one before what's held. A latest page that starts
    /// past `after`, the newest seq held when it was asked for, with more before it, leaves a gap: what's held before it
    /// goes and the page is taken as the first, so items that aren't adjacent never show as if they were.
    func page(_ timeline: AgentTimeline, older: Bool, after: Int? = nil) {
        if !older, timeline.hasOlder, let after, let first = timeline.items.first?.seq, after < first {
            items.removeAll { $0.seq < first }
            index = Dictionary(uniqueKeysWithValues: items.enumerated().map { ($1.id, $0) })
            unchanged = 0
            primed = false
            loaded = false
        }
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
            let background = inBackground(session.transcript.turns[...]).map(\.call)
            if background != self.background { self.background = background }
            let agents = spawned(session.transcript)
            if agents != self.agents { self.agents = agents }
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

    /// The chat or a sub-agent of it came on screen: fetch what's missing, tell pocketd it's viewed, keep the queue moving.
    public func appear() {
        viewers += 1
        guard viewers == 1 else { return }
        send(.agentView(.init(agentIds: [agentId])))
        open()
        if let cwd = summary?.cwd, let pieces = files?.takeAdditions(for: agentId, in: cwd), !pieces.isEmpty {
            draft.text = FilesStore.draft(draft.text, adding: pieces)
        }
        ticker?.cancel()
        ticker = Task { @MainActor [weak self] in
            while !Task.isCancelled {
                try? await Task.sleep(for: .seconds(1))
                self?.flush()
            }
        }
    }

    public func disappear() {
        viewers = max(viewers - 1, 0)
        guard viewers == 0 else { return }
        ticker?.cancel()
        ticker = nil
        // The app is still in front: without `front`, pocketd would take leaving the chat for leaving the app and push.
        if let view = HostModel.sceneView(active: true, viewing: [], front: hasCap(Caps.front)) { send(.agentView(view)) }
    }

    func open() {
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
        let after = items.last?.seq
        request(.agentTimeline(.init(agentId: agentId, limit: Self.page))) { [weak self] reply in
            if case .agentTimeline(let t) = reply { self?.page(t, older: false, after: after) }
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

    private var info: ProviderInfo? { providers?.providers.first { $0.id == provider } }

    public var capabilities: Capabilities { ChatModel.capabilities(provider, info?.chat) }

    /// Whether the access pill is a menu: pocketd lets the phone set a chat agent's access, and the agent has more than Full.
    public var setsAccess: Bool { summary?.chatOnly == true && hasCap(Caps.access) && info?.neverAsks != true }

    public var observeOnly: Bool { Composer.observeOnly(scopes) }

    public var canApprove: Bool { Composer.canApprove(scopes) }

    /// The ask the agent waits on, as the chat shows it.
    public var ask: Ask? { requests.first.map(ChatModel.ask) }

    public var feedback: Bool { requests.first?.feedback ?? false }

    public var mode: Composer.Mode { Composer.mode(observeOnly: observeOnly, canApprove: canApprove, ask: ask, closed: closed) }

    public var busy: Bool { summary.map { Composer.busy($0, session) } ?? false }

    public var stopping: Bool { session.outgoing.stopping }

    public var action: Action { ChatModel.action(working: busy, stopping: stopping, empty: draft.isEmpty) }

    public var uploading: Bool { draft.images.contains { $0.path == nil && !$0.failed } }

    public var uploadFailed: Bool { draft.images.contains(where: \.failed) }

    public func hasCap(_ cap: String) -> Bool { caps.contains(cap) }

    // MARK: Sending

    /// Sends the draft unless the agent can't take it now. While the agent works a send goes into the running turn; behind
    /// a stop, or to `queue`, it waits for the turn to end. While images upload it arms instead, and goes once they land.
    public func send(queue: Bool = false) {
        guard mode == .prompt, online, !armed, !uploadFailed, let agent = summary else { return }
        if uploading {
            armed = true
            armedQueue = queue
            return
        }
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
        // New Session takes what follows `/new` as its prompt; the draft stays, so backing out of it loses nothing.
        if case (.local(.new), _, let arg)? = command { return startNewSession(arg) }
        draft = Draft()
        sentAt = clock()
        if case (.local(let local), _, let arg)? = command { return runLocal(local, arg) }
        let sent = command != nil ? text : suggest.withMcpContext(agentId, provider, text)
        if skipsQueue(typed) {
            _ = prompt(sent)
            return
        }
        deliver(sent, steer: steer, busy: busy)
    }

    /// Sends `text` as Send would a typed draft, leaving the draft alone; false when the agent can't take it now.
    @discardableResult
    public func send(text: String) -> Bool {
        guard mode == .prompt, online, summary != nil else { return false }
        let busy = busy
        if busy && !capabilities.sendWhileWorking { return false }
        sentAt = clock()
        deliver(text, steer: busy && !stopping, busy: busy)
        return true
    }

    func deliver(_ text: String, steer: Bool, busy: Bool) {
        if !steer { return push(text, busy: busy) }
        let id = session.outgoing.steerText(text, now: clock())
        session.outgoing.sentAs(id, prompt(text))
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
        case (.new, let prompt): startNewSession(prompt)
        case (.btw, ""): break
        case (.model, let model): settings(model: model)
        case (.effort, let effort): settings(effort: effort)
        case (.compact, ""): compact()
        case (.compact, _): notice = Composer.compactTakesNoInstructions(provider: provider)
        case (.btw, let question): ask(btw: question)
        case (.mcp, let query): draft.text = mcpTag + query
        }
    }

    /// Asks the screen to open New Session where this chat's agent runs.
    func startNewSession(_ prompt: String) {
        let prompt = prompt.trimmingCharacters(in: .whitespacesAndNewlines)
        newSession = NewSession(cwd: summary?.cwd, prompt: prompt.isEmpty ? nil : prompt)
    }

    public func openedNewSession() {
        asked = newSession.map { ($0, draft) }
        newSession = nil
    }

    /// A session started. One with the prompt `/new` asked for is the New Session this chat opened, so the draft that
    /// asked for it goes, unless it was changed since.
    public func started(prompt: String?) {
        guard let asked, asked.session.prompt == prompt else { return }
        self.asked = nil
        if draft == asked.draft { draft = Draft() }
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
        guard online else { return }
        session.outgoing.stop(now: clock())
        request(.agentInterrupt(.init(agentId: agentId))) { _ in } failed: { [weak self] _ in
            self?.session.outgoing.stopRefused()
        }
        scheduleFlush()
    }

    /// Stops the sub-agents a chat agent left in the background; no turn runs, so nothing waits on a stop.
    public func stopBackground() { request(.agentInterrupt(.init(agentId: agentId))) { _ in } }

    /// What the send button does when pressed: disarms an armed send, stops a working agent with an empty draft unless
    /// a send just went, else sends.
    public func press() {
        if armed {
            armed = false
        } else if action == .stop {
            if !Composer.guardsStop(sentAt: sentAt, now: clock()) { stop() }
        } else {
            send()
        }
    }

    public func sendNow() {
        guard online else { return }
        guard session.outgoing.sendNow(now: clock()) else { return }
        request(.agentSendNow(.init(agentId: agentId))) { _ in }
        scheduleFlush()
    }

    public func compact() { request(.agentCompact(.init(agentId: agentId))) { _ in } }

    /// Sends a queued send into the turn now.
    public func steerQueued(_ id: Int) {
        guard online else { return }
        guard let text = session.outgoing.steer(id, now: clock()) else { return }
        session.outgoing.sentAs(id, prompt(text))
        steers += 1
        scheduleFlush()
    }

    /// Pulls a queued send back into the composer, under what's typed.
    public func editQueued(_ id: Int) {
        guard let text = session.outgoing.withdraw(id) else { return }
        restore(text)
        scheduleFlush()
    }

    /// Puts a send back in the composer under what's typed, its images as tiles that already landed.
    func restore(_ prompt: String) {
        let (text, paths) = splitImages(prompt)
        draft.text = Queue.editing(draft.text, text)
        let held = Set(draft.images.compactMap(\.path))
        draft.images += paths.filter { !held.contains($0) }.map {
            Attachment(id: UUID().uuidString, name: FileText.baseName($0), path: $0, thumbnail: thumbnails[$0])
        }
    }

    public func removeQueued(_ id: Int) {
        _ = session.outgoing.withdraw(id)
        scheduleFlush()
    }

    /// Puts a send that wasn't delivered back in the composer.
    public func retry(_ id: Int) {
        guard let text = session.outgoing.retry(id) else { return }
        restore(text)
        scheduleFlush()
    }

    public func resume() {
        session.outgoing.resume(session.transcript)
        scheduleFlush()
    }

    /// Re-sends the prompt that set a reply going, leaving the draft alone.
    public func retryTurn(_ turn: Int) {
        guard mode == .prompt, online, summary != nil, session.transcript.turns.indices.contains(turn),
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

    /// Answers the first ask, once: a second answer waits until this one fails.
    func resolve(_ decision: Decision, message: String? = nil, answers: [String: String]? = nil) {
        guard online, let request = requests.first, answering != request.requestId else { return }
        let id = request.requestId
        answering = id
        self.request(.permissionResolve(.init(requestId: id, decision: decision, message: message, answers: answers))) { _ in } failed: {
            [weak self] error in
            guard let self else { return }
            if answering == id { answering = nil }
            notice = error
            // The ask still stands, so its questions can be sent again.
            if wizard?.requestId == id { wizard?.sent = false }
        }
    }

    /// The question wizard's step: a pick, the next page, back or skip.
    public func wizard(_ change: (inout Wizard) -> Wizard.Step) {
        guard var w = wizard, !w.sent else { return }
        let step = change(&w)
        if case .done = step, !online { return }
        wizard = w
        if case .done(let answers) = step {
            wizard?.sent = true
            resolve(.allow, answers: answers)
        }
    }

    /// Skips the questions: a deny that stops the agent's turn.
    public func skipQuestions() {
        guard wizard?.sent == false, online else { return }
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
        request(.agentSettings(.init(agentId: agentId, model: model, effort: effort, access: access))) { _ in } failed: { [weak self] error in
            self?.notice = error
        }
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
            if action != nil { suggest.mcpActFailed(agent) } else { suggest.mcpListed(agent, .failure(.init(error ?? "")), authUrl: "", acted: false) }
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
    /// runs, and a server the agent can't use yet is signed in to, reconnected or enabled. Returns where the cursor goes
    /// when the draft changed.
    @discardableResult
    public func pick(_ ix: Int, cursor: Int) -> Int? {
        guard let rows = suggestions(cursor: cursor) else { return nil }
        let text = draft.text
        let utf8 = Array(text.utf8)
        func around(_ at: Int, _ put: String) -> (text: String, cursor: Int) {
            let head = String(decoding: utf8[..<at], as: UTF8.self) + put
            return (head + String(decoding: utf8[min(cursor, utf8.count)...], as: UTF8.self), head.utf8.count)
        }
        let put: (text: String, cursor: Int)
        suggest.selected = 0
        switch rows {
        case .servers(let at, _, .success(let all)?, let pickable) where ix < pickable:
            if !Mcp.usable(all[ix]) {
                act(on: all[ix])
                return nil
            }
            put = replaced(text, at, cursor, mcpTag + all[ix].name)
        case .commands(let at, _, let list) where list.indices.contains(ix) && !startsDraft(text, at):
            put = list[ix].kind == .local(.mcp) ? around(at, mcpTag) : replaced(text, at, cursor, "/\(list[ix].name)")
        case .commands(_, _, let list) where list.indices.contains(ix):
            let arg = slash(text)?.arg ?? ""
            guard case .local(let local) = adapter.command(list[ix].name, arg) else {
                put = named(list[ix].name, arg)
                break
            }
            // New Session keeps the draft, as a typed `/new` does.
            if local != .new { draft.text = "" }
            runLocal(local, arg)
            return draft.text.utf8.count
        case .files(let at, _, let paths) where paths.indices.contains(ix):
            put = replaced(text, at, cursor, "@" + paths[ix])
        default:
            return nil
        }
        draft.text = put.text
        return put.cursor
    }

    // MARK: Images

    /// Attaches an image and uploads it; the send waits until it lands.
    public func attach(_ data: Data, name: String, thumbnail: Data? = nil) {
        let id = UUID().uuidString
        draft.images.append(Attachment(id: id, name: name, thumbnail: thumbnail))
        uploadData[id] = data
        upload(id)
    }

    /// Sends a failed image again.
    public func retryUpload(_ id: String) {
        guard uploadData[id] != nil else { return }
        mark(id) {
            $0.failed = false
            $0.reason = nil
        }
        upload(id)
    }

    public func detach(_ id: String) {
        draft.images.removeAll { $0.id == id }
        uploadData[id] = nil
        sendArmed()
    }

    func upload(_ id: String) {
        guard let data = uploadData[id], let name = draft.images.first(where: { $0.id == id })?.name else { return }
        guard let files else { return failed(id, DaemonError.offline) }
        Task { @MainActor [weak self] in
            do {
                let path = try await files.upload(data, name: name)
                self?.landed(id, path)
            } catch {
                self?.failed(id, error)
            }
        }
    }

    func landed(_ id: String, _ path: String) {
        uploadData[id] = nil
        mark(id) { $0.path = path }
        if let thumbnail = draft.images.first(where: { $0.id == id })?.thumbnail { thumbnails[path] = thumbnail }
        sendArmed()
    }

    func failed(_ id: String, _ error: any Error) {
        let reason = FilesStore.describe(error, mac: macName)
        mark(id) {
            $0.failed = true
            $0.reason = reason
        }
        sendArmed()
    }

    /// An armed send goes once every image has landed, and disarms as soon as one fails.
    func sendArmed() {
        guard armed else { return }
        if uploadFailed {
            armed = false
        } else if !uploading {
            armed = false
            send(queue: armedQueue)
        }
    }

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

    /// Everything the agent did is held and folded: the first page came, no older one is left and no refold is due.
    public var whole: Bool { loaded && !hasOlder && !flushing }

    /// Pages in the items before the oldest held while the sub-agent the call `callId` spawned isn't among them, as for its
    /// screen opened on a chat holding only its latest page.
    public func seek(_ callId: String) {
        guard !flushing, ChatModel.subAgent(session.transcript, callId) == nil else { return }
        loadOlder()
    }

    /// Asks the chat to show the call its main agent spawned the sub-agent `callId` under.
    public func reveal(_ callId: String) { revealing = spawnedUnder(session.transcript, callId) }

    public func revealed() { revealing = nil }

    // MARK: Plumbing

    func persist() {
        guard let defaults else { return }
        var kept = draft
        // A whole photo passed as a thumbnail would stall typing, as the draft is encoded on each keystroke.
        kept.images = kept.images.filter { $0.path != nil }.map { var a = $0; if (a.thumbnail?.count ?? 0) > Self.thumbnailMax { a.thumbnail = nil }; return a }
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

    /// Sends a request; `done` takes its reply, `failed` why it failed as the reader sees it (nil once cancelled), both
    /// on the main actor.
    func request(
        _ message: ClientMessage, done: @escaping @MainActor (ServerMessage) -> Void,
        failed: @escaping @MainActor (String?) -> Void = { _ in }
    ) {
        // Even offline the answer comes later, as a caller keys a send by what this returns before it fails.
        guard let connection else {
            Task { @MainActor [macName] in failed(ErrorText.describe(DaemonError.offline, mac: macName)) }
            return
        }
        Task { @MainActor [macName] in
            do {
                done(try await connection.request(message))
            } catch {
                failed(ErrorText.describe(error, mac: macName))
            }
        }
    }
}
