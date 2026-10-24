import Wire

/// The colour of a lane: the checked-out branch, its upstream, the base branch, or one of the rotating colours.
public enum LaneColor: Hashable, Sendable {
    case head, upstream, base
    case lane(Int)
}

/// A line of the graph heading down to commit `sha`.
public struct Lane: Hashable, Sendable {
    public var sha: String
    public var color: LaneColor
    public init(sha: String, color: LaneColor) {
        self.sha = sha
        self.color = color
    }
}

/// One commit's row: the lanes entering it from above, those leaving it below, and the column and colour of its dot.
public struct GraphRow: Hashable, Sendable {
    public var input: [Lane]
    public var output: [Lane]
    public var column: Int
    public var color: LaneColor
}

/// The lanes left open by the commits pushed so far, so the next page of history continues the graph
/// (crates/git/src/graph.rs, itself VS Code's `scmHistory.ts`).
public struct Lanes: Sendable {
    private var open: [Lane] = []
    private var next = 0

    public init() {}

    /// Lays out the next commit, newest first; `label` colours the lane leaving a branch tip.
    public mutating func push(_ sha: String, parents: [String], label: (String) -> LaneColor? = { _ in nil }) -> GraphRow {
        let input = open
        let column = input.firstIndex { $0.sha == sha } ?? input.count
        var output: [Lane] = []
        output.reserveCapacity(input.count + parents.count)
        var placed = false
        for lane in input {
            if lane.sha != sha {
                output.append(lane)
            } else if !placed, let first = parents.first {
                output.append(Lane(sha: first, color: label(sha) ?? lane.color))
                placed = true
            }
        }
        for (i, p) in parents.enumerated().dropFirst(placed ? 1 : 0) {
            output.append(Lane(sha: p, color: (i == 0 ? label(sha) : label(p)) ?? nextColor()))
        }
        let dot = parents.isEmpty ? (column < input.count ? input[column] : nil) : (column < output.count ? output[column] : nil)
        open = output
        return GraphRow(input: input, output: output, column: column, color: dot?.color ?? .head)
    }

    private mutating func nextColor() -> LaneColor {
        defer { next += 1 }
        return .lane(next)
    }
}

/// The tips the graph colours lanes by (git.rs `Tips::color`).
public struct GraphLabels: Hashable, Sendable {
    public var tips: LogTips
    public init(_ tips: LogTips) { self.tips = tips }

    public func color(_ sha: String) -> LaneColor? {
        if sha == tips.head { return .head }
        if sha == tips.upstream { return .upstream }
        if sha == tips.base { return .base }
        return nil
    }
}

/// A page of history laid out: one row per commit, continuing the lanes of the pages before it.
public struct GraphLayout: Sendable {
    public private(set) var rows: [GraphRow] = []
    /// The most lanes any row holds, for the graph column's width.
    public private(set) var width = 1
    private var lanes = Lanes()
    private let labels: GraphLabels

    public init(tips: LogTips) { labels = GraphLabels(tips) }

    public mutating func append(_ commits: [LogCommit]) {
        rows.reserveCapacity(rows.count + commits.count)
        for c in commits {
            let row = lanes.push(c.sha, parents: c.parents, label: labels.color)
            width = max(width, row.input.count, row.output.count, row.column + 1)
            rows.append(row)
        }
    }
}

/// The strokes of one graph row, as lane columns: lanes passing by, lanes ending in the commit's dot, and lanes
/// leaving it for its parents. Draw `through` from the top edge to the bottom, `into` from the top to the dot,
/// `out` from the dot to the bottom.
public struct GraphEdges: Hashable, Sendable {
    public struct Through: Hashable, Sendable {
        public var from: Int
        public var to: Int
        public var color: LaneColor
    }

    public struct End: Hashable, Sendable {
        public var column: Int
        public var color: LaneColor
    }

    public var through: [Through] = []
    public var into: [End] = []
    public var out: [End] = []

    /// Follows `Lanes.push`: lanes other than the commit's keep their order, its first parent takes the place of the
    /// first lane that waited for it, and further parents open lanes at the end.
    public init(_ row: GraphRow, sha: String, parents: Int) {
        var j = 0
        var placed = false
        for (k, lane) in row.input.enumerated() {
            if lane.sha != sha {
                through.append(Through(from: k, to: j, color: lane.color))
                j += 1
            } else {
                into.append(End(column: k, color: lane.color))
                if !placed, parents > 0 {
                    out.append(End(column: j, color: row.output[j].color))
                    j += 1
                    placed = true
                }
            }
        }
        while j < row.output.count {
            out.append(End(column: j, color: row.output[j].color))
            j += 1
        }
    }
}
