/// A piece of work on the Board. `state` says where it is in its life; `status`, while it is live, what its session is doing.
public struct Ticket: Codable, Hashable, Sendable, Identifiable {
    public var id: String
    public var title: String
    public var prompt: String
    public var provider: ProviderId
    public var folder: String
    /// The branch its worktree starts from; absent is the project's default.
    public var base: String?
    public var state: String
    public var order: Int
    public var status: String?
    public var why: String?
    public var agentId: String?
    public var terminalId: String?
    public var worktree: String?
    /// Unix milliseconds.
    public var createdAt: Int
    public var changedAt: Int
    public var closedAt: Int?
    public var closedWhy: String?
    public var relaunch: Bool?
    /// Its short ID on the Board.
    public var number: Int?
    /// 1 (urgent) to 4 (low); absent is none.
    public var priority: Int?
    public var labels: [String]?

    public init(
        id: String, title: String, prompt: String, provider: ProviderId, folder: String, base: String? = nil, state: String, order: Int,
        status: String? = nil, why: String? = nil, agentId: String? = nil, terminalId: String? = nil, worktree: String? = nil,
        createdAt: Int, changedAt: Int, closedAt: Int? = nil, closedWhy: String? = nil, relaunch: Bool? = nil, number: Int? = nil,
        priority: Int? = nil, labels: [String]? = nil
    ) {
        self.id = id
        self.title = title
        self.prompt = prompt
        self.provider = provider
        self.folder = folder
        self.base = base
        self.state = state
        self.order = order
        self.status = status
        self.why = why
        self.agentId = agentId
        self.terminalId = terminalId
        self.worktree = worktree
        self.createdAt = createdAt
        self.changedAt = changedAt
        self.closedAt = closedAt
        self.closedWhy = closedWhy
        self.relaunch = relaunch
        self.number = number
        self.priority = priority
        self.labels = labels
    }
}

/// Every ticket, sent whole on each change.
public struct Board: Codable, Hashable, Sendable {
    public var tickets: [Ticket]
    /// How many tickets may be live at once.
    public var limit: Int
    public init(tickets: [Ticket], limit: Int) {
        self.tickets = tickets
        self.limit = limit
    }
}

extension ClientMessage {
    /// A ticket as `board.save` sends it; no `id` creates.
    public struct TicketDraft: Codable, Hashable, Sendable {
        public var id: String?
        public var title: String
        public var prompt: String
        public var provider: ProviderId
        public var folder: String
        public var base: String?
        public var priority: Int?
        public var labels: [String]?
        public init(
            id: String? = nil, title: String, prompt: String, provider: ProviderId, folder: String, base: String? = nil, priority: Int? = nil,
            labels: [String]? = nil
        ) {
            self.id = id
            self.title = title
            self.prompt = prompt
            self.provider = provider
            self.folder = folder
            self.base = base
            self.priority = priority
            self.labels = labels
        }
    }

    public struct BoardSave: Codable, Hashable, Sendable {
        public var ticket: TicketDraft
        /// Starts it once saved.
        public var start: Bool?
        public init(ticket: TicketDraft, start: Bool? = nil) {
            self.ticket = ticket
            self.start = start
        }
    }

    public struct TicketRef: Codable, Hashable, Sendable {
        public var ticketId: String
        public init(ticketId: String) { self.ticketId = ticketId }
    }

    public struct BoardMove: Codable, Hashable, Sendable {
        public var ticketId: String
        /// `todo` or `queued`.
        public var column: String
        /// The ticket it goes before; absent or empty is last.
        public var before: String?
        public init(ticketId: String, column: String, before: String? = nil) {
            self.ticketId = ticketId
            self.column = column
            self.before = before
        }
    }

    public struct BoardLimit: Codable, Hashable, Sendable {
        public var limit: Int
        public init(limit: Int) { self.limit = limit }
    }
}
