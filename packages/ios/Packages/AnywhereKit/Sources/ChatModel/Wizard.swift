/// An ask's questions, a page each: single-select advances on its own, number keys pick.
public struct Wizard: Hashable, Sendable {
    /// How long a single-select pick shows before the wizard moves on.
    public static let autoAdvance: Seconds = 0.22
    public static let placeholder = "Something else…"
    public static let placeholderPicked = "Something else, or leave blank to use your pick"

    public enum Step: Hashable, Sendable {
        case stay, autoAdvance
        case done([String: String])
    }

    public let requestId: String
    public let questions: [Question]
    public private(set) var page = 0
    var picked: [[Int]]
    var typed: [String]
    /// Whether the last page change went forward, so the new page slides in from that side.
    public private(set) var forward = true
    /// Set once the answers or the skip went out; the ask stays pending until the next poll, and mustn't send twice.
    public var sent = false

    public init(_ ask: Ask) {
        requestId = ask.requestId
        questions = ask.questions
        picked = Array(repeating: [], count: ask.questions.count)
        typed = Array(repeating: "", count: ask.questions.count)
    }

    public var question: Question { questions[page] }
    var last: Bool { page + 1 >= questions.count }
    public func isPicked(_ option: Int) -> Bool { picked[page].contains(option) }
    public var pageHasPick: Bool { !picked[page].isEmpty }
    public func canAdvance(typedEmpty: Bool) -> Bool { pageHasPick || !typedEmpty }
    public var primary: String { last ? "Submit" : "Next" }
    public var currentTyped: String { typed[page] }
    public var currentPlaceholder: String { pageHasPick ? Self.placeholderPicked : Self.placeholder }

    /// A pick over typed words clears them and the picks they hid, since typed words win.
    /// The last page's pick waits for Submit, so an answer is never sent by a stray key.
    public mutating func select(_ option: Int) -> Step {
        let q = questions[page]
        guard option < q.options.count else { return .stay }
        if !typed[page].trimmingWhitespace().isEmpty {
            typed[page] = ""
            picked[page] = []
        }
        if !q.multi {
            picked[page] = [option]
            return last ? .stay : .autoAdvance
        }
        if let at = picked[page].firstIndex(of: option) { picked[page].remove(at: at) } else { picked[page].append(option) }
        return .stay
    }

    /// The option key `n` picks: 1–9 the first nine, of those the page has.
    public func keyOption(_ n: Int) -> Int? {
        (1...9).contains(n) && n - 1 < questions[page].options.count ? n - 1 : nil
    }

    public mutating func setTyped(_ text: String) { typed[page] = text }

    public mutating func advance() -> Step {
        if last { return .done(answers()) }
        page += 1
        forward = true
        return .stay
    }

    /// Steps back a page; false on the first, which has none to go back to.
    public mutating func back() -> Bool {
        guard page > 0 else { return false }
        page -= 1
        forward = false
        return true
    }

    /// Each answer by its question's text; typed words win over picks, and several picks join with ", ".
    func answers() -> [String: String] {
        var out: [String: String] = [:]
        for (ix, q) in questions.enumerated() {
            let typed = typed[ix].trimmingWhitespace()
            out[q.question] = typed.isEmpty ? picked[ix].map { q.options[$0].label }.joined(separator: ", ") : typed
        }
        return out
    }
}

/// Small caps tracked out with hair spaces, as an eyebrow label.
public func trackedUpper(_ text: String) -> String {
    text.uppercased().map(String.init).joined(separator: "\u{200A}")
}

/// A question's first line as its title, and the rest, if any, as markdown under it.
public func titleAndBody(_ question: String) -> (title: String, body: String?) {
    let q = question.trimmingWhitespace()
    guard let nl = q.firstIndex(of: "\n") else { return (q, nil) }
    let body = q[q.index(after: nl)...].trimmingWhitespace()
    return (q[..<nl].trimmingWhitespace(), body.isEmpty ? nil : body)
}
