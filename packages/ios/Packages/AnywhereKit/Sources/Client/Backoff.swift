/// Redial delays: doubling from `first` up to `cap`, each picked in the upper half of its step so many phones don't redial in step.
public struct Backoff: Sendable, Equatable {
    public let first: Double
    public let cap: Double
    public private(set) var step: Double

    public init(first: Double = 0.25, cap: Double = 8) {
        self.first = first
        self.cap = cap
        step = first
    }

    /// Seconds to wait before the next dial; `random` is in 0...1.
    public mutating func next(random: Double = .random(in: 0...1)) -> Double {
        let delay = step * (0.5 + 0.5 * min(max(random, 0), 1))
        step = min(step * 2, cap)
        return delay
    }

    public mutating func reset() { step = first }
}
