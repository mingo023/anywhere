import Foundation

/// One Live Activity per Mac. pocketd pushes `ContentState`; it carries counts, statuses and agent ids only.
public struct MacActivity: Codable, Hashable, Sendable {
    /// The Mac's `hello.ok` serverId.
    public var mac: String
    public var macName: String

    public init(mac: String, macName: String) {
        self.mac = mac
        self.macName = macName
    }

    public struct ContentState: Codable, Hashable, Sendable {
        public var working: Int
        public var needsYou: Int
        public var unseen: Int
        /// The session to show: the oldest Needs you, else the longest working, else the oldest Failed, else the oldest Done.
        public var top: Top?

        public init(working: Int, needsYou: Int, unseen: Int, top: Top? = nil) {
            self.working = working
            self.needsYou = needsYou
            self.unseen = unseen
            self.top = top
        }

        /// Anything working or waiting on the user, which keeps the activity going.
        public var isBusy: Bool { working > 0 || needsYou > 0 }

        /// Ranks this Mac's activity against others' for the Dynamic Island, as pocketd does.
        public var relevance: Double {
            if needsYou > 0 { return 100 }
            if top?.status == Top.failed { return 75 }
            if working > 0 { return 50 }
            return 10
        }
    }

    public struct Top: Codable, Hashable, Sendable {
        public static let needsYou = "needsYou"
        public static let working = "working"
        public static let failed = "failed"
        public static let done = "done"

        /// The agent id.
        public var slot: String
        /// needsYou, working, failed or done.
        public var status: String
        /// Unix seconds as a Double, because ActivityKit decodes a `Date` from 2001.
        public var sinceUnix: Double
        public var verb: String?
        public var subject: String?
        /// Allow and Deny can answer it from the Lock Screen.
        public var answerable: Bool
        public var request: String?

        public init(slot: String, status: String, sinceUnix: Double, verb: String? = nil, subject: String? = nil, answerable: Bool, request: String? = nil) {
            self.slot = slot
            self.status = status
            self.sinceUnix = sinceUnix
            self.verb = verb
            self.subject = subject
            self.answerable = answerable
            self.request = request
        }

        public var since: Date { Date(timeIntervalSince1970: sinceUnix) }
    }

    /// The shown activities to end so each Mac keeps one. pocketd updates only the one whose token it got last (`held`), so
    /// any other passes its stale date and says the Mac is offline. The one kept: a current one over a stale one, then the
    /// one held, then the one updated last, by its stale date.
    public static func extras(_ shown: [(id: String, mac: String, stale: Bool, staleDate: Date?)], held: Set<String>) -> Set<String> {
        let kept = Dictionary(grouping: shown) { $0.mac }.values.compactMap { same in
            same.max { a, b in
                if a.stale != b.stale { return a.stale }
                if held.contains(a.id) != held.contains(b.id) { return held.contains(b.id) }
                return (a.staleDate ?? .distantPast) < (b.staleDate ?? .distantPast)
            }?.id
        }
        return Set(shown.map(\.id)).subtracting(kept)
    }

    /// Before starting a Mac's activity, its shown ones to end, or nil to start none. A current one stays, as pocketd keeps
    /// it up to date; a stale one, which nothing may update, gives way to a fresh one. A renewal replaces only a stale one.
    public static func replaced(_ shown: [(id: String, stale: Bool)], renewing: Bool) -> [String]? {
        if shown.contains(where: { !$0.stale }) || (renewing && shown.isEmpty) { return nil }
        return shown.map(\.id)
    }
}

#if canImport(ActivityKit) && os(iOS)
    import ActivityKit

    extension MacActivity: ActivityAttributes {}
#endif
