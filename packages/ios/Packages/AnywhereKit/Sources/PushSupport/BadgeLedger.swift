import Foundation

/// Each pocketd knows only its own Mac's count, so the badge is the sum of the last count each Mac sent, kept in the App Group.
public struct BadgeLedger: @unchecked Sendable {
    let defaults: UserDefaults
    static let key = "badges"

    public init(defaults: UserDefaults) {
        self.defaults = defaults
    }

    /// Records a Mac's count and returns the sum across Macs.
    public func record(mac: String, count: Int) -> Int {
        var counts = defaults.dictionary(forKey: Self.key) as? [String: Int] ?? [:]
        counts[mac] = max(count, 0)
        defaults.set(counts, forKey: Self.key)
        return counts.values.reduce(0, +)
    }

    public func forget(mac: String) {
        var counts = defaults.dictionary(forKey: Self.key) as? [String: Int] ?? [:]
        counts[mac] = nil
        defaults.set(counts, forKey: Self.key)
    }
}
