import DiffModel

/// Lines picked to quote, from where the press started to where it ends. In a diff a pick stays inside one hunk.
public struct LinePick: Hashable, Sendable {
    public private(set) var anchor: Int
    public private(set) var end: Int

    public enum Edge: Sendable {
        case top, bottom
    }

    public init(_ line: Int) {
        anchor = line
        end = line
    }

    public var lines: ClosedRange<Int> { min(anchor, end)...max(anchor, end) }

    /// Stretches the pick to `line`, if `allowed` from its anchor; true when it moved.
    public mutating func drag(to line: Int, allowed: (Int, Int) -> Bool = { _, _ in true }) -> Bool {
        guard line != end, allowed(anchor, line) else { return false }
        end = line
        return true
    }

    /// Takes hold of one edge, as a handle drag starts: the other becomes the anchor, so `drag` moves only this one,
    /// even past the anchor and back.
    public mutating func grab(_ edge: Edge) {
        (anchor, end) = edge == .top ? (lines.upperBound, lines.lowerBound) : (lines.lowerBound, lines.upperBound)
    }

    /// Whether no hunk header sits between rows `a` and `b` of a diff, both included.
    public static func sameHunk(_ lines: [UnifiedLine], _ a: Int, _ b: Int) -> Bool {
        lines.indices.contains(a) && lines.indices.contains(b) && !lines[min(a, b)...max(a, b)].contains { $0.kind == .hunk }
    }
}
