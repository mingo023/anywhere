import Client
import Observation
import Wire

/// Where a Mac's connection stands, for banners and the Macs list.
public enum LinkState: Hashable, Sendable {
    case connecting
    case online
    /// Dropped; redialing after `seconds`.
    case retrying(seconds: Double)
    /// Ended for good: revoked, a side to update, or stopped.
    case ended(CloseOutcome)
}

/// One paired Mac: its connection and the stores its messages feed.
@MainActor
@Observable
public final class HostModel: Identifiable {
    public nonisolated let id: String
    public let credentials: Credentials
    @ObservationIgnored public let connection: DaemonConnection
    public private(set) var link = LinkState.connecting
    public private(set) var hello: HelloOk?

    public let sessions: SessionsStore
    public let workspace: WorkspaceStore
    public let terminals: TerminalStore
    public let git: GitStore
    public let github: GitHubStore
    public let files: FilesStore
    public let push: PushStore
    /// One per open chat, keyed by agent id.
    public private(set) var chats: [String: ChatStore] = [:]

    @ObservationIgnored private var listener: Task<Void, Never>?

    public init(credentials: Credentials, connection: DaemonConnection? = nil) {
        id = credentials.deviceId
        self.credentials = credentials
        let connection = connection ?? DaemonConnection(config: .init(host: credentials.host, token: credentials.deviceToken, clientId: credentials.clientId))
        self.connection = connection
        sessions = SessionsStore(connection: connection)
        workspace = WorkspaceStore(connection: connection)
        terminals = TerminalStore(connection: connection)
        git = GitStore(connection: connection)
        github = GitHubStore(connection: connection)
        files = FilesStore(connection: connection)
        push = PushStore(connection: connection)
    }

    /// The Mac's name: what it calls itself once it has said hello, else what pairing saved.
    public var name: String { hello?.hostname ?? credentials.macName }

    /// Starts the connection; its events reach the stores in batches of up to 16 ms.
    public func start() {
        guard listener == nil else { return }
        let connection = connection
        let coalescer = Coalescer<ConnectionEvent> { [weak self] batch in self?.apply(batch) }
        listener = Task {
            for await event in connection.events { await coalescer.add(event) }
            await coalescer.finish()
        }
        Task { await connection.start() }
    }

    /// In the background the phone stops being Seen at once, so pocketd pushes what ends meanwhile; in front it redials now.
    public func scene(active: Bool) {
        let connection = connection
        Task {
            if active {
                await connection.reconnectNow()
            } else {
                _ = try? await connection.send(.agentView(.init(agentIds: [])))
            }
        }
    }

    public func stop() {
        Task { [connection] in await connection.stop() }
    }

    /// The chat store of an agent, made on first open.
    public func chat(_ agentId: String) -> ChatStore {
        if let chat = chats[agentId] { return chat }
        let chat = ChatStore(agentId: agentId, connection: connection, defaults: .standard, draftScope: id)
        chat.seed(sessions.session(agentId)?.summary, requests: sessions.session(agentId)?.requests ?? [], hello: hello)
        chats[agentId] = chat
        return chat
    }

    public func closeChat(_ agentId: String) {
        chats[agentId] = nil
    }

    private var stores: [any HostStore] {
        [sessions, workspace, terminals, git, github, files, push] + Array(chats.values)
    }

    /// Hands a batch of events to the stores, in order.
    public func apply(_ batch: [ConnectionEvent]) {
        for event in batch {
            switch event {
            case .connected(let ok):
                hello = ok
                link = .online
                for store in stores { store.connected(ok) }
            case .message(let message):
                route(message)
            case .terminal(let frame):
                _ = terminals.apply(frame)
            case .closed(let outcome):
                switch outcome {
                case .retrying(let seconds): link = .retrying(seconds: seconds)
                default: link = .ended(outcome)
                }
                for store in stores { store.disconnected() }
            }
        }
    }

    /// Every store sees every message and takes what it owns; a chat sees only its own agent's.
    @discardableResult
    func route(_ message: ServerMessage) -> Bool {
        var taken = false
        for store in [sessions, workspace, terminals, git, github, files, push] as [any HostStore] {
            taken = store.apply(message) || taken
        }
        if let agentId = message.agentId {
            if let chat = chats[agentId] { taken = chat.apply(message) || taken }
        } else {
            for chat in chats.values { taken = chat.apply(message) || taken }
        }
        return taken
    }
}

extension ServerMessage {
    /// The one agent a message is about, when it is about one.
    public var agentId: String? {
        switch self {
        case .agentUpdate(let m): m.agent.id
        case .agentCommands(let m): m.agentId
        case .agentMcp(let m): m.agentId
        case .agentBtw(let m): m.agentId
        case .agentStream(let m): m.agentId
        case .agentTimeline(let m): m.agentId
        case .permissionRequest(let m): m.request.agentId
        case .namingFailed(let m): m.agentId
        default: nil
        }
    }
}
