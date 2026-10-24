import Foundation
import Network
import Wire

/// One WebSocket to one Mac's pocketd. It says hello, correlates replies to requests by id, and redials with backoff
/// until the Mac refuses this phone or `stop()` is called. Runs on its own serial queue, which Network's callbacks share.
public actor DaemonConnection {
    public struct Config: Sendable {
        public var host: String
        public var token: String?
        public var clientId: String
        public var caps: [String]
        public var helloTimeout: Duration
        public var requestTimeout: Duration
        public var backoff: Backoff

        public init(
            host: String, token: String?, clientId: String, caps: [String] = Caps.phone, helloTimeout: Duration = .seconds(10),
            requestTimeout: Duration = .seconds(20), backoff: Backoff = Backoff()
        ) {
            self.host = host
            self.token = token
            self.clientId = clientId
            self.caps = caps
            self.helloTimeout = helloTimeout
            self.requestTimeout = requestTimeout
            self.backoff = backoff
        }
    }

    public nonisolated let events: AsyncStream<ConnectionEvent>
    public nonisolated let config: Config

    public nonisolated var unownedExecutor: UnownedSerialExecutor { queue.asUnownedSerialExecutor() }

    private enum State {
        case idle
        case connecting
        case online(HelloOk)
        case stopped
    }

    private struct Pending {
        let continuation: CheckedContinuation<ServerMessage, any Error>
        let timeout: Task<Void, Never>
    }

    private let queue = DispatchSerialQueue(label: "dev.mingo.anywhere.connection")
    private let continuation: AsyncStream<ConnectionEvent>.Continuation
    private var state = State.idle
    private var connection: NWConnection?
    /// Bumped on every dial, so callbacks from an older socket are ignored.
    private var generation = 0
    private var backoff: Backoff
    private var retry: Task<Void, Never>?
    private var helloTimer: Task<Void, Never>?
    private var pending: [String: Pending] = [:]
    /// Requests that timed out or were cancelled; their late replies are dropped, not reported.
    private var abandoned: Set<String> = []
    private var lastId = 0
    private let helloId = "hello"
    /// The last error code before hello.ok, which says why the Mac is about to close.
    private var refusal: ErrorCode?
    /// A receive is waiting on the socket, and will report how it closed.
    private var receiving = false

    public init(config: Config) {
        self.config = config
        backoff = config.backoff
        (events, continuation) = AsyncStream.makeStream(of: ConnectionEvent.self, bufferingPolicy: .unbounded)
    }

    public var isOnline: Bool {
        if case .online = state { true } else { false }
    }

    public var hello: HelloOk? {
        if case .online(let ok) = state { ok } else { nil }
    }

    public func start() {
        guard case .idle = state else { return }
        dial()
    }

    /// Dials now instead of waiting out the backoff: the app came to the front or the network changed.
    public func reconnectNow() {
        switch state {
        case .stopped, .online: return
        case .connecting where retry == nil: return
        case .idle, .connecting:
            backoff.reset()
            dial()
        }
    }

    /// Closes for good; the stream ends after `.closed(.stopped)`.
    public func stop() {
        guard !isStopped else { return }
        state = .stopped
        drop()
        failPending(.closed)
        continuation.yield(.closed(.stopped))
        continuation.finish()
    }

    /// Sends a request and waits for its reply. A reply of `error` throws `DaemonError.server`.
    /// Interim replies (launch progress) go to `events`; the request waits for the one after them.
    public func request(_ message: ClientMessage, timeout: Duration? = nil) async throws -> ServerMessage {
        guard isOnline else { throw DaemonError.offline }
        let id = nextId()
        try write(ClientFrame(id: id, message))
        let limit = timeout ?? config.requestTimeout
        return try await withTaskCancellationHandler {
            try await withCheckedThrowingContinuation { continuation in
                let timer = Task { [weak self] in
                    try? await Task.sleep(for: limit)
                    await self?.abandon(id, .timedOut)
                }
                pending[id] = Pending(continuation: continuation, timeout: timer)
            }
        } onCancel: {
            Task { await self.abandon(id, .cancelled) }
        }
    }

    /// Sends without waiting; any reply arrives on `events`. Returns the message's id.
    @discardableResult
    public func send(_ message: ClientMessage) throws -> String {
        guard isOnline else { throw DaemonError.offline }
        let id = nextId()
        try write(ClientFrame(id: id, message))
        return id
    }

    private var isStopped: Bool {
        if case .stopped = state { true } else { false }
    }

    private func nextId() -> String {
        lastId += 1
        return "c\(lastId)"
    }

    private func dial() {
        drop()
        guard let url = DaemonEndpoint.url(host: config.host) else {
            continuation.yield(.closed(.updatePhone))
            state = .stopped
            continuation.finish()
            return
        }
        generation += 1
        let generation = generation
        state = .connecting
        refusal = nil

        let parameters: NWParameters = url.scheme == "wss" ? .tls : .tcp
        let ws = NWProtocolWebSocket.Options()
        ws.autoReplyPing = true
        ws.maximumMessageSize = 64 << 20
        parameters.defaultProtocolStack.applicationProtocols.insert(ws, at: 0)
        let connection = NWConnection(to: .url(url), using: parameters)
        self.connection = connection
        connection.stateUpdateHandler = { [weak self] state in
            self?.assumeIsolated { $0.changed(state, generation) }
        }
        connection.start(queue: queue)
        helloTimer = Task { [weak self, limit = config.helloTimeout] in
            try? await Task.sleep(for: limit)
            await self?.helloTimedOut(generation)
        }
    }

    private func changed(_ state: NWConnection.State, _ generation: Int) {
        guard generation == self.generation else { return }
        switch state {
        case .ready:
            let hello = ClientMessage.Hello(token: config.token, clientId: config.clientId, caps: config.caps)
            try? write(ClientFrame(id: helloId, .hello(hello)))
            receive(generation)
        case .failed, .cancelled:
            guard receiving else { return lost(closeCode: nil) }
            // The armed receive completes next with the close frame's code, if one came; let it decide.
            queue.asyncAfter(deadline: .now() + 0.1) { [weak self] in
                self?.assumeIsolated { $0.lostUnlessSettled(generation) }
            }
        case .waiting:
            // No route yet (offline, VPN down): count it as a drop so backoff paces the redials.
            lost(closeCode: nil)
        default:
            break
        }
    }

    private func lostUnlessSettled(_ generation: Int) {
        guard generation == self.generation else { return }
        lost(closeCode: nil)
    }

    private func receive(_ generation: Int) {
        receiving = true
        connection?.receiveMessage { [weak self] content, context, _, error in
            self?.assumeIsolated { $0.received(content, context, error, generation) }
        }
    }

    private func received(_ content: Data?, _ context: NWConnection.ContentContext?, _ error: NWError?, _ generation: Int) {
        guard generation == self.generation else { return }
        receiving = false
        let metadata = context?.protocolMetadata(definition: NWProtocolWebSocket.definition) as? NWProtocolWebSocket.Metadata
        if error != nil, metadata?.opcode != .close {
            lost(closeCode: nil)
            return
        }
        switch metadata?.opcode {
        case .text?:
            if let content { text(content) }
        case .binary?:
            if let content, let frame = try? TerminalFrame(parsing: content) {
                continuation.yield(.terminal(frame))
            }
        case .close?:
            let reason = content.flatMap { String(data: $0, encoding: .utf8) }.flatMap { $0.isEmpty ? nil : ErrorCode(rawValue: $0) }
            if refusal == nil { refusal = reason }
            lost(closeCode: metadata.map { Self.code($0.closeCode) })
            return
        default:
            break
        }
        guard generation == self.generation, connection != nil else { return }
        receive(generation)
    }

    private func text(_ data: Data) {
        guard let frame = try? ServerFrame.decode(data) else {
            if !isOnline { finish(.updateMac) }
            return
        }
        guard isOnline else { return preHello(frame) }
        if let id = frame.id {
            if let waiting = pending[id], !frame.message.isInterim {
                pending[id] = nil
                waiting.timeout.cancel()
                if case .error(let error) = frame.message {
                    waiting.continuation.resume(throwing: DaemonError.server(error))
                } else {
                    waiting.continuation.resume(returning: frame.message)
                }
                return
            }
            if abandoned.contains(id) {
                if !frame.message.isInterim { abandoned.remove(id) }
                return
            }
        }
        continuation.yield(.message(frame.message))
    }

    /// Before hello.ok, only the hello's answer counts: a refusal, or an old Mac that can't read the hello.
    private func preHello(_ frame: ServerFrame) {
        switch frame.message {
        case .helloOk(let ok):
            if ok.serverTooOld { return finish(.updateMac) }
            helloTimer?.cancel()
            state = .online(ok)
            backoff.reset()
            continuation.yield(.connected(ok))
        case .error(let error):
            if let code = error.code {
                refusal = code
                if let end = CloseOutcome.final(closeCode: nil, errorCode: code) { finish(end) }
            } else if error.message == malformedMessage {
                finish(.updateMac)
            }
        default:
            break
        }
    }

    private func helloTimedOut(_ generation: Int) {
        guard generation == self.generation, case .connecting = state else { return }
        lost(closeCode: nil)
    }

    private func lost(closeCode: Int?) {
        guard !isStopped else { return }
        if let end = CloseOutcome.final(closeCode: closeCode, errorCode: refusal) { return finish(end) }
        drop()
        failPending(.closed)
        state = .idle
        let delay = backoff.next()
        continuation.yield(.closed(.retrying(seconds: delay)))
        state = .connecting
        retry = Task { [weak self] in
            try? await Task.sleep(for: .seconds(delay))
            guard !Task.isCancelled else { return }
            await self?.redial()
        }
    }

    private func redial() {
        guard case .connecting = state else { return }
        retry = nil
        dial()
    }

    /// The Mac refused this phone for good: no more redials.
    private func finish(_ outcome: CloseOutcome) {
        guard !isStopped else { return }
        state = .stopped
        drop()
        failPending(.closed)
        continuation.yield(.closed(outcome))
        continuation.finish()
    }

    private func drop() {
        retry?.cancel()
        retry = nil
        helloTimer?.cancel()
        helloTimer = nil
        generation += 1
        receiving = false
        guard let connection else { return }
        self.connection = nil
        connection.stateUpdateHandler = nil
        let metadata = NWProtocolWebSocket.Metadata(opcode: .close)
        metadata.closeCode = .protocolCode(.normalClosure)
        let context = NWConnection.ContentContext(identifier: "close", metadata: [metadata])
        connection.send(content: nil, contentContext: context, isComplete: true, completion: .contentProcessed { _ in connection.cancel() })
    }

    private func failPending(_ error: DaemonError) {
        let waiting = pending
        pending = [:]
        abandoned = []
        for (_, p) in waiting {
            p.timeout.cancel()
            p.continuation.resume(throwing: error)
        }
    }

    private func abandon(_ id: String, _ error: DaemonError) {
        guard let p = pending.removeValue(forKey: id) else { return }
        p.timeout.cancel()
        abandoned.insert(id)
        p.continuation.resume(throwing: error)
    }

    private func write(_ frame: ClientFrame) throws {
        guard let connection else { throw DaemonError.offline }
        let data: Data
        do { data = try frame.encoded() } catch { throw DaemonError.encoding("\(error)") }
        let metadata = NWProtocolWebSocket.Metadata(opcode: .text)
        let context = NWConnection.ContentContext(identifier: "text", metadata: [metadata])
        connection.send(content: data, contentContext: context, isComplete: true, completion: .idempotent)
    }

    private static func code(_ code: NWProtocolWebSocket.CloseCode) -> Int {
        switch code {
        case .protocolCode(let c): Int(c.rawValue)
        case .applicationCode(let c): Int(c)
        case .privateCode(let c): Int(c)
        @unknown default: 0
        }
    }
}
