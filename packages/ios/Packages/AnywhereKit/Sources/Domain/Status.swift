/// Where an agent stands from the user's view, most urgent first.
public enum Status: Int, Comparable, Hashable, Sendable, CaseIterable {
    case needsYou
    case failed
    case done
    case working
    case idle

    /// The status a session shows. An agent that isn't attached shows only terminal activity, never Needs you or Done.
    /// `status` is pocketd's word: needsYou, done, working, idle or closed.
    public init(status: String, failed: Bool, attached: Bool) {
        guard attached else {
            self = .idle
            return
        }
        switch status {
        case "needsYou": self = .needsYou
        case "done": self = failed ? .failed : .done
        case "working": self = .working
        default: self = .idle
        }
    }

    /// Wants a look: Needs you, Failed or Done.
    public var wantsLook: Bool { self <= .done }

    public var label: String? {
        switch self {
        case .needsYou: "Needs you"
        case .failed: "Failed"
        case .done: "Done"
        case .working: "Working"
        case .idle: nil
        }
    }

    public static func < (a: Status, b: Status) -> Bool { a.rawValue < b.rawValue }
}

/// What ordering needs from a session.
public struct Ranked: Hashable, Sendable {
    public var id: String
    public var status: Status
    /// Unix milliseconds of its last status step.
    public var updatedAt: Int
    public var createdAt: Int

    public init(id: String, status: Status, updatedAt: Int, createdAt: Int) {
        self.id = id
        self.status = status
        self.updatedAt = updatedAt
        self.createdAt = createdAt
    }
}

public enum Order {
    /// The sessions that want a look, in the order to handle them: Needs you, then Failed, then Done, each oldest status change first.
    public static func upNext(_ sessions: [Ranked]) -> [String] {
        sessions.filter(\.status.wantsLook)
            .sorted { a, b in
                if a.status != b.status { return a.status < b.status }
                if a.updatedAt != b.updatedAt { return a.updatedAt < b.updatedAt }
                return a.id < b.id
            }
            .map(\.id)
    }

    /// Every session, newest first.
    public static func recent(_ sessions: [Ranked]) -> [String] {
        sessions.sorted { a, b in a.createdAt != b.createdAt ? a.createdAt > b.createdAt : a.id < b.id }.map(\.id)
    }

    /// "2 need you · 1 failed · 3 working", or the session count when none of them wants anything.
    public static func summary(_ statuses: [Status]) -> String? {
        guard !statuses.isEmpty else { return nil }
        let count = { (s: Status) in statuses.filter { $0 == s }.count }
        let needs = count(.needsYou)
        let parts = [
            needs > 0 ? "\(needs) \(needs == 1 ? "needs" : "need") you" : nil,
            count(.failed) > 0 ? "\(count(.failed)) failed" : nil,
            count(.done) > 0 ? "\(count(.done)) done" : nil,
            count(.working) > 0 ? "\(count(.working)) working" : nil,
        ].compactMap(\.self)
        if !parts.isEmpty { return parts.joined(separator: " · ") }
        return statuses.count == 1 ? "1 session" : "\(statuses.count) sessions"
    }
}
