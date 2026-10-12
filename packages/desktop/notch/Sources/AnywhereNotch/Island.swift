/// A permission ask the island offers to answer.
struct Ask: Codable, Equatable {
    var agent: String
    var request: String
    var provider: String
    /// The provider's short name, "Claude".
    var name: String
    /// The provider's icon, as SVG.
    var icon: String
    var title: String
    var place: String
    var verb: String
    var subject: String
    var shell: Bool
    var feedback: Bool
}

enum Answer: Equatable {
    case allow, deny, reply(String)
}

enum Stage: Equatable {
    case compact, open, reply
    /// The answered ask, showing how it went until `next`.
    case done(Answer)
}

/// The asks waiting in the island, the first one showing.
struct Island: Equatable {
    private(set) var asks: [Ask] = []
    private(set) var stage = Stage.compact
    /// The desktop lists an answered ask until it hears back, which mustn't show it again.
    private var answered: Set<String> = []

    var current: Ask? { asks.first }

    private var isDone: Bool {
        if case .done = stage { return true }
        return false
    }

    /// Shows `pending`, the desktop's asks; the answered one stays, showing how it went, until `next`.
    mutating func show(_ pending: [Ask]) {
        let showing = current?.request
        answered.formIntersection(pending.map(\.request))
        let waiting = pending.filter { !answered.contains($0.request) }
        asks = isDone ? Array(asks.prefix(1)) + waiting : waiting
        if !isDone && current?.request != showing {
            stage = .compact
        }
    }

    mutating func open() {
        if stage == .compact && current != nil {
            stage = .open
        }
    }

    /// One step back: the reply field to the ask, the ask to the compact island.
    mutating func close() {
        switch stage {
        case .reply: stage = .open
        case .open: stage = .compact
        default: break
        }
    }

    /// Back to the compact island from the ask or its reply.
    mutating func collapse() {
        if stage == .open || stage == .reply {
            stage = .compact
        }
    }

    mutating func reply() {
        if stage == .open && current?.feedback == true {
            stage = .reply
        }
    }

    /// Answers the showing ask, once; returns it.
    mutating func answer(_ answer: Answer) -> Ask? {
        guard !isDone, let ask = current else { return nil }
        stage = .done(answer)
        answered.insert(ask.request)
        return ask
    }

    /// Lets the answered ask go and shows the next.
    mutating func next() {
        if isDone {
            asks.removeFirst()
            stage = .compact
        }
    }
}
