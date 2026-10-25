import Foundation

public struct Terminal: Codable, Hashable, Sendable, Identifiable {
    public var id: String
    public var cwd: String
    public var cols: Int
    public var rows: Int
    /// The foreground process text.
    public var title: String?
    public var localId: String?
    /// The agent running in it now, if any.
    public var agentId: String?
    public var exited: Bool?

    public init(id: String, cwd: String, cols: Int, rows: Int, title: String? = nil, localId: String? = nil, agentId: String? = nil, exited: Bool? = nil) {
        self.id = id
        self.cwd = cwd
        self.cols = cols
        self.rows = rows
        self.title = title
        self.localId = localId
        self.agentId = agentId
        self.exited = exited
    }
}

/// Every terminal: the reply to `terminal.list`, and pushed whole on spawn, exit and title change.
public struct TerminalList: Codable, Hashable, Sendable {
    public var terminals: [Terminal]
    public init(terminals: [Terminal]) { self.terminals = terminals }
}

/// An attach took; a SNAPSHOT frame follows, then OUTPUT frames in PTY order.
public struct TerminalAttached: Codable, Hashable, Sendable {
    public var terminalId: String
    public var cols: Int
    public var rows: Int
    public init(terminalId: String, cols: Int, rows: Int) {
        self.terminalId = terminalId
        self.cols = cols
        self.rows = rows
    }
}

public struct TerminalResized: Codable, Hashable, Sendable {
    public var terminalId: String
    public var cols: Int
    public var rows: Int
    public init(terminalId: String, cols: Int, rows: Int) {
        self.terminalId = terminalId
        self.cols = cols
        self.rows = rows
    }
}

/// The terminal's shell ended; its attach ends with it.
public struct TerminalExited: Codable, Hashable, Sendable {
    public var terminalId: String
    public var code: Int
    public init(terminalId: String, code: Int) {
        self.terminalId = terminalId
        self.code = code
    }
}

public struct TerminalCreated: Codable, Hashable, Sendable {
    public var requestId: String
    public var terminalId: String
    public init(requestId: String, terminalId: String) {
        self.requestId = requestId
        self.terminalId = terminalId
    }
}

extension ClientMessage {
    /// `terminal.attach`, `terminal.detach` and `terminal.close`.
    public struct TerminalRef: Codable, Hashable, Sendable {
        public var terminalId: String
        public init(terminalId: String) { self.terminalId = terminalId }
    }

    /// Keystrokes; `data` travels as base64.
    public struct TerminalInput: Codable, Hashable, Sendable {
        public var terminalId: String
        public var data: Data
        public init(terminalId: String, data: Data) {
            self.terminalId = terminalId
            self.data = data
        }
    }

    public struct TerminalResize: Codable, Hashable, Sendable {
        public var terminalId: String
        public var cols: Int
        public var rows: Int
        public init(terminalId: String, cols: Int, rows: Int) {
            self.terminalId = terminalId
            self.cols = cols
            self.rows = rows
        }
    }

    /// A login shell; `cwd` must be a tree.
    public struct TerminalCreate: Codable, Hashable, Sendable {
        public var requestId: String
        public var cwd: String?
        public var localId: String?
        public var cols: Int?
        public var rows: Int?
        public init(requestId: String, cwd: String? = nil, localId: String? = nil, cols: Int? = nil, rows: Int? = nil) {
            self.requestId = requestId
            self.cwd = cwd
            self.localId = localId
            self.cols = cols
            self.rows = rows
        }
    }
}
