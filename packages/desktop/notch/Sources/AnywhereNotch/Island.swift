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
    var questions: [Question] = []
}

/// One of AskUserQuestion's questions.
struct Question: Codable, Equatable {
    var question: String
    var header: String
    var options: [Choice]
    var multiSelect: Bool
}

struct Choice: Codable, Equatable {
    var label: String
    var description: String
}

enum Answer: Equatable {
    case allow, deny, reply(String)
    /// AskUserQuestion's answers, keyed by question.
    case questions([String: String])
}

/// The labels picked for an ask's questions, answered one at a time.
struct Picks: Equatable {
    let questions: [Question]
    private(set) var page = 0
    private var chosen: [[String]]

    init(_ questions: [Question]) {
        self.questions = questions
        chosen = questions.map { _ in [] }
    }

    /// The question showing; nil once the last is answered.
    var question: Question? { questions.indices.contains(page) ? questions[page] : nil }
    var isLast: Bool { page == questions.count - 1 }
    var isAnswered: Bool { question != nil && !chosen[page].isEmpty }
    var isDone: Bool { !questions.isEmpty && page == questions.count }

    func isChosen(_ label: String) -> Bool { question != nil && chosen[page].contains(label) }

    /// Toggles `label` on a multi-select question; on another, chooses it and moves on.
    mutating func pick(_ label: String) {
        guard let question else { return }
        if question.multiSelect {
            chosen[page] = chosen[page].contains(label) ? chosen[page].filter { $0 != label } : chosen[page] + [label]
        } else {
            chosen[page] = [label]
            next()
        }
    }

    /// Answers the showing question with `text` in place of its options, as Claude's own dialog's "Other" does, and moves on.
    mutating func type(_ text: String) {
        guard question != nil else { return }
        chosen[page] = [text]
        next()
    }

    mutating func next() {
        if isAnswered { page += 1 }
    }

    mutating func back() {
        if page > 0 { page -= 1 }
    }

    /// Keyed by question; a multi-select's labels in the options' order, then any typed answer, joined as Claude's own dialog does.
    var answers: [String: String] {
        var out: [String: String] = [:]
        for (q, labels) in zip(questions, chosen) {
            let options = q.options.map(\.label)
            out[q.question] = (options.filter(labels.contains) + labels.filter { !options.contains($0) }).joined(separator: ", ")
        }
        return out
    }
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
    /// The showing ask's picks, kept while it shows.
    private(set) var picks = Picks([])
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
            picks = Picks(current?.questions ?? [])
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

    /// Picks `label` for the showing question; answers the ask, returning it, once the last is answered.
    mutating func pick(_ label: String) -> Ask? {
        guard stage == .open else { return nil }
        picks.pick(label)
        return answerIfDone()
    }

    /// Sends the reply field's `text`: on a question, its answer, answering the ask, returning it, past the last; otherwise a reply in place of allowing.
    mutating func send(_ text: String) -> Ask? {
        guard stage == .reply else { return nil }
        if picks.question == nil {
            return answer(.reply(text))
        }
        picks.type(text)
        stage = .open
        return answerIfDone()
    }

    /// On from a multi-select question; answers the ask, returning it, past the last.
    mutating func nextQuestion() -> Ask? {
        guard stage == .open else { return nil }
        picks.next()
        return answerIfDone()
    }

    private mutating func answerIfDone() -> Ask? {
        picks.isDone ? answer(.questions(picks.answers)) : nil
    }

    mutating func previousQuestion() {
        if stage == .open { picks.back() }
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
            picks = Picks(current?.questions ?? [])
        }
    }
}
