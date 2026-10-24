import Client
import Foundation
import Observation
import Wire

/// Where an attached terminal's bytes go: the emulator a terminal screen draws.
@MainActor
public protocol TerminalSink: AnyObject {
    /// A SNAPSHOT: reset the screen at the terminal's size, then feed.
    func replace(cols: Int, rows: Int, with bytes: Data)
    func feed(_ bytes: Data)
    func resize(cols: Int, rows: Int)
}

/// One terminal a screen has open. Its own object, so a change redraws that screen alone.
@MainActor
@Observable
public final class TerminalAttachment: Identifiable {
    public enum State: Hashable, Sendable {
        case attaching
        case live
        /// The connection dropped; it attaches again on reconnect.
        case offline
        case exited(code: Int)
        /// pocketd no longer lists it.
        case gone
    }

    public let id: String
    public internal(set) var state = State.attaching
    public internal(set) var cols: Int
    public internal(set) var rows: Int
    /// A keystroke was refused because the terminal's agent waits on a permission; answer it from its card.
    public internal(set) var askOpen = false
    @ObservationIgnored weak var sink: (any TerminalSink)?

    init(id: String, cols: Int, rows: Int, sink: any TerminalSink) {
        self.id = id
        self.cols = cols
        self.rows = rows
        self.sink = sink
    }

    var running: Bool {
        switch state {
        case .attaching, .live, .offline: true
        case .exited, .gone: false
        }
    }
}

/// One Mac's terminals, and the open screens its binary frames feed.
@MainActor
@Observable
public final class TerminalStore: HostStore {
    /// Every running terminal, by folder.
    public private(set) var terminals: [Terminal] = []
    /// pocketd has answered a `terminal.list` since connecting.
    public private(set) var listed = false
    /// The Mac speaks terminal.v1.
    public private(set) var available = true
    /// The owner turned phone.terminals off: screens may watch, not type.
    public private(set) var writesOff = false
    public private(set) var attachments: [String: TerminalAttachment] = [:]
    /// The Mac's Locals, where a new terminal may open.
    public private(set) var locals: [Local] = []

    @ObservationIgnored let connection: DaemonConnection?
    /// Fire-and-forget messages, sent one after another so keystrokes keep their order.
    @ObservationIgnored private var post: (ClientMessage) -> Void = { _ in }
    /// The terminal each recent attach and keystroke named, by message id, so a refusal finds its screen.
    @ObservationIgnored private var sent: [String: String] = [:]
    @ObservationIgnored private var sentOrder: [String] = []

    public init(connection: DaemonConnection? = nil) {
        self.connection = connection
        let (stream, outbox) = AsyncStream.makeStream(of: ClientMessage.self)
        post = { outbox.yield($0) }
        Task { [weak self, connection] in
            for await message in stream {
                guard let id = try? await connection?.send(message) else { continue }
                self?.note(id, message)
            }
        }
    }

    /// A store whose sends go to `post`, numbered c1, c2… as a connection numbers them: for tests and the gallery.
    public init(post: @escaping (ClientMessage) -> Void) {
        connection = nil
        var last = 0
        self.post = { [unowned self] message in
            post(message)
            last += 1
            note("c\(last)", message)
        }
    }

    private static let keptIds = 256

    private func note(_ id: String, _ message: ClientMessage) {
        let terminalId: String
        switch message {
        case .terminalAttach(let ref): terminalId = ref.terminalId
        case .terminalInput(let input): terminalId = input.terminalId
        default: return
        }
        sent[id] = terminalId
        sentOrder.append(id)
        if sentOrder.count > Self.keptIds {
            sent[sentOrder.removeFirst()] = nil
        }
    }

    public func terminal(_ id: String) -> Terminal? { terminals.first { $0.id == id } }

    public func apply(_ message: ServerMessage) -> Bool {
        switch message {
        case .terminalList(let list):
            return replace(list.terminals)
        case .terminalAttached(let a):
            guard let open = attachments[a.terminalId] else { return false }
            open.cols = a.cols
            open.rows = a.rows
            open.state = .live
            return true
        case .terminalResized(let r):
            if let at = terminals.firstIndex(where: { $0.id == r.terminalId }) {
                terminals[at].cols = r.cols
                terminals[at].rows = r.rows
            }
            guard let open = attachments[r.terminalId] else { return true }
            open.cols = r.cols
            open.rows = r.rows
            open.sink?.resize(cols: r.cols, rows: r.rows)
            return true
        case .terminalExited(let e):
            guard let open = attachments[e.terminalId] else { return false }
            open.state = .exited(code: e.code)
            return true
        case .localList(let list):
            guard list.locals != locals else { return false }
            locals = list.locals
            return true
        case .error(let e) where e.code == .terminalsOff:
            guard !writesOff else { return false }
            writesOff = true
            return true
        case .error(let e):
            return refused(e)
        case .permissionResolved:
            var changed = false
            for open in attachments.values where open.askOpen {
                open.askOpen = false
                changed = true
            }
            return changed
        case .hostChanged(let changed):
            return follow(changed.host)
        default:
            return false
        }
    }

    /// Takes in a terminal's bytes; true when an open screen took them.
    public func apply(_ frame: TerminalFrame) -> Bool {
        guard let open = attachments[frame.terminalId], open.running, let sink = open.sink else { return false }
        switch frame.kind {
        case .snapshot: sink.replace(cols: open.cols, rows: open.rows, with: frame.payload)
        case .output: sink.feed(frame.payload)
        }
        return true
    }

    /// The list and every open screen come back on their own, each screen from a fresh snapshot.
    public func connected(_ hello: HelloOk) {
        available = hello.has(Caps.terminal)
        // An older Mac says terminals are off only by refusing a write, and the owner may have turned them on while this was away.
        writesOff = false
        sent = [:]
        sentOrder = []
        if let host = hello.host { _ = follow(host) }
        guard available else { return }
        post(.terminalList)
        for open in attachments.values where open.running {
            open.state = .attaching
            post(.terminalAttach(.init(terminalId: open.id)))
        }
    }

    public func disconnected() {
        listed = false
        for open in attachments.values where open.running { open.state = .offline }
    }

    public func refresh() { post(.terminalList) }

    /// Opens a screen on a terminal: a snapshot replaces whatever `sink` showed, then its output follows.
    @discardableResult
    public func attach(_ id: String, sink: any TerminalSink) -> TerminalAttachment {
        let size = terminal(id).map { ($0.cols, $0.rows) } ?? (80, 24)
        let open = attachments[id] ?? TerminalAttachment(id: id, cols: size.0, rows: size.1, sink: sink)
        open.sink = sink
        open.state = .attaching
        attachments[id] = open
        post(.terminalAttach(.init(terminalId: id)))
        return open
    }

    public func detach(_ id: String) {
        guard attachments.removeValue(forKey: id) != nil else { return }
        post(.terminalDetach(.init(terminalId: id)))
    }

    public func input(_ id: String, _ bytes: Data) {
        guard !bytes.isEmpty else { return }
        post(.terminalInput(.init(terminalId: id, data: bytes)))
    }

    /// Fits the terminal to the phone; every screen on it, the desktop's too, reflows.
    public func resize(_ id: String, cols: Int, rows: Int) {
        post(.terminalResize(.init(terminalId: id, cols: min(max(cols, 1), 1000), rows: min(max(rows, 1), 1000))))
    }

    /// Opens a login shell in a tree or a Local; returns its terminal id.
    public func create(cwd: String?, localId: String?, cols: Int? = nil, rows: Int? = nil) async throws -> String {
        guard let connection else { throw DaemonError.offline }
        let create = ClientMessage.TerminalCreate(requestId: UUID().uuidString, cwd: cwd, localId: localId, cols: cols, rows: rows)
        guard case .terminalCreated(let created) = try await connection.request(.terminalCreate(create)) else {
            throw DaemonError.encoding("terminal.create got another reply")
        }
        return created.terminalId
    }

    public func close(_ id: String) async throws {
        guard let connection else { throw DaemonError.offline }
        _ = try await connection.request(.terminalClose(.init(terminalId: id)))
    }

    /// Every project and its worktrees, for picking where a new terminal opens.
    public func projects() async throws -> [Project] {
        guard let connection else { throw DaemonError.offline }
        guard case .projectList(let list) = try await connection.request(.projectList) else { return [] }
        return list.projects
    }

    /// Any refused attach means its terminal ended (an older pocketd gives no code); a keystroke refused with ask_open
    /// means the terminal's agent waits on a permission.
    private func refused(_ error: ServerError) -> Bool {
        guard let id = error.replyTo, let terminalId = sent.removeValue(forKey: id), let open = attachments[terminalId] else { return false }
        if error.code == .askOpen {
            guard !open.askOpen else { return false }
            open.askOpen = true
            return true
        }
        guard open.state == .attaching || error.code == .terminalEnded else { return false }
        open.state = .gone
        return true
    }

    /// Follows the Mac's phone.terminals setting; nil from a pocketd too old to say.
    private func follow(_ host: HostState) -> Bool {
        guard let on = host.terminals, writesOff == on else { return false }
        writesOff = !on
        return true
    }

    private func replace(_ list: [Terminal]) -> Bool {
        var changed = !listed
        listed = true
        if list != terminals {
            terminals = list
            changed = true
        }
        let ids = Set(list.map(\.id))
        // Only a live attach: a list sent before an attach was answered may predate its terminal.
        for open in attachments.values where open.state == .live && !ids.contains(open.id) {
            open.state = .gone
            changed = true
        }
        return changed
    }
}
