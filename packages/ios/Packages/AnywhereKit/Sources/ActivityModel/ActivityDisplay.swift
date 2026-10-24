import Foundation

/// What each Live Activity region says, as plain values, so the widget and the gallery show the same words.
public enum ActivityDisplay {
    /// The glyph a state leads with: its most urgent session.
    public enum Glyph: Hashable, Sendable {
        case needsYou, failed, working, done, idle
    }

    /// The compact trailing region.
    public enum Trailing: Hashable, Sendable {
        /// "!" and how many need you.
        case needs(Int)
        /// Elapsed time of the longest working turn.
        case timer(since: Date)
        /// Sessions done and not yet seen.
        case unseen(Int)
        case none
    }

    public static func glyph(_ s: MacActivity.ContentState) -> Glyph {
        if s.needsYou > 0 { return .needsYou }
        switch s.top?.status {
        case MacActivity.Top.failed: return .failed
        case MacActivity.Top.working: return .working
        case MacActivity.Top.done: return .done
        default: return s.working > 0 ? .working : .idle
        }
    }

    public static func trailing(_ s: MacActivity.ContentState) -> Trailing {
        if s.needsYou > 0 { return .needs(s.needsYou) }
        if s.working > 0, let top = s.top, top.status == MacActivity.Top.working { return .timer(since: top.since) }
        if s.unseen > 0 { return .unseen(s.unseen) }
        return .none
    }

    /// The minimal region's badge: how many need you, else how many work.
    public static func badge(_ s: MacActivity.ContentState) -> Int? {
        if s.needsYou > 0 { return s.needsYou }
        if s.working > 0 { return s.working }
        return nil
    }

    public static func label(_ status: String) -> String {
        switch status {
        case MacActivity.Top.needsYou: "Needs you"
        case MacActivity.Top.failed: "Failed"
        case MacActivity.Top.done: "Done"
        case MacActivity.Top.working: "Working"
        default: "Idle"
        }
    }

    /// The top session's ask, "Run · npm test", or its status when it asks nothing.
    public static func ask(_ top: MacActivity.Top) -> String {
        let parts = [top.verb, top.subject].compactMap { $0?.isEmpty == false ? $0 : nil }
        return parts.isEmpty ? label(top.status) : parts.joined(separator: " · ")
    }

    /// "2 need you · 1 working · 3 done", or "All done" once nothing is left to look at.
    public static func summary(_ s: MacActivity.ContentState) -> String {
        let parts = [
            s.needsYou > 0 ? "\(s.needsYou) \(s.needsYou == 1 ? "needs" : "need") you" : nil,
            s.working > 0 ? "\(s.working) working" : nil,
            s.unseen > 0 ? "\(s.unseen) done" : nil,
        ].compactMap(\.self)
        return parts.isEmpty ? "All done" : parts.joined(separator: " · ")
    }

    /// "+2 more" beside the top session on the Lock Screen.
    public static func more(_ s: MacActivity.ContentState) -> String? {
        let others = s.working + s.needsYou + s.unseen - (s.top == nil ? 0 : 1)
        return others > 0 ? "+\(others) more" : nil
    }

    public static func offline(_ macName: String) -> String { "\(macName) is offline" }

    /// What the whole activity opens.
    public static func link(mac: String, top: MacActivity.Top?) -> URL {
        URL(string: top.map { "anywhere://mac/\(mac)/agent/\($0.slot)" } ?? "anywhere://mac/\(mac)")!
    }
}

/// Session titles and places the app keeps in the App Group while connected, so the widget names the top session
/// without pocketd putting titles in a push.
public struct ActivityTitles: @unchecked Sendable {
    public struct Entry: Codable, Hashable, Sendable {
        public var title: String
        public var place: String
        public var provider: String

        public init(title: String, place: String, provider: String = "claude") {
            self.title = title
            self.place = place
            self.provider = provider
        }
    }

    static let key = "activity.titles"
    public static let fallback = Entry(title: "A session", place: "", provider: "")

    let defaults: UserDefaults

    public init(defaults: UserDefaults) {
        self.defaults = defaults
    }

    /// Replaces one Mac's entries; only what changed is written.
    public func save(mac: String, _ entries: [String: Entry]) {
        var all = load()
        guard all[mac] != entries else { return }
        all[mac] = entries
        if let data = try? JSONEncoder().encode(all) { defaults.set(data, forKey: Self.key) }
    }

    public func entry(mac: String, slot: String?) -> Entry {
        guard let slot else { return Self.fallback }
        return load()[mac]?[slot] ?? Self.fallback
    }

    func load() -> [String: [String: Entry]] {
        defaults.data(forKey: Self.key).flatMap { try? JSONDecoder().decode([String: [String: Entry]].self, from: $0) } ?? [:]
    }
}
