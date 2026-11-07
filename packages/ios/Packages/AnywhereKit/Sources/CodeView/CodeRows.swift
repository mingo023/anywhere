/// The rows a list of lines draws as: one per line, or with wrapping, each line cut to fit a number of columns.
public struct CodeRows: Sendable, Equatable {
    public struct Row: Hashable, Sendable {
        public let line: Int
        /// The part of the line the row shows, in UTF-16 offsets; nil for the whole line.
        public let range: Range<Int>?
        /// The row after a line cut short, offering the rest of it.
        public var more = false

        /// Whether the row starts its line, so it carries the line's number.
        public var first: Bool { !more && (range?.lowerBound ?? 0) == 0 }
    }

    private let wrapped: [Row]?
    private let lines: Int

    /// One row per line, each whole.
    public init(lineCount: Int) {
        wrapped = nil
        lines = lineCount
    }

    /// Each line cut into rows of at most `columns`, after its last space when one fits. A line longer than `limit`
    /// columns stops there and gets a `more` row, unless it's in `expanded`. A wide character takes two columns.
    public init(_ texts: [String], columns: Int, limit: Int? = CodeStyle.maxChars, expanded: Set<Int> = []) {
        let columns = max(columns, 1)
        var rows: [Row] = []
        rows.reserveCapacity(texts.count)
        for (line, text) in texts.enumerated() {
            let cut = Self.cut(text, columns: columns, limit: expanded.contains(line) ? nil : limit)
            for r in cut.ranges { rows.append(Row(line: line, range: r)) }
            if cut.more { rows.append(Row(line: line, range: nil, more: true)) }
        }
        wrapped = rows
        lines = texts.count
    }

    public var count: Int { wrapped?.count ?? lines }

    public subscript(_ i: Int) -> Row { wrapped?[i] ?? Row(line: i, range: nil) }

    /// The index of `line`'s first row.
    public func first(of line: Int) -> Int {
        guard let wrapped else { return line }
        var lo = 0
        var hi = wrapped.count
        while lo < hi {
            let mid = (lo + hi) / 2
            if wrapped[mid].line < line { lo = mid + 1 } else { hi = mid }
        }
        return lo
    }

    /// The indices of `line`'s rows.
    public func rows(of line: Int) -> Range<Int> {
        guard wrapped != nil else { return line..<line + 1 }
        return first(of: line)..<first(of: line + 1)
    }

    /// A line's rows as UTF-16 ranges, and whether it stopped at `limit` with more to show.
    static func cut(_ text: String, columns: Int, limit: Int?) -> (ranges: [Range<Int>], more: Bool) {
        var ranges: [Range<Int>] = []
        var start = 0
        var at = 0
        var used = 0
        var total = 0
        var afterSpace: (offset: Int, used: Int)?
        func step(_ length: Int, width: Int, space: Bool) -> Bool {
            if let limit, total + width > limit { return false }
            if used + width > columns, at > start {
                let end = afterSpace.map { $0.offset > start ? $0.offset : at } ?? at
                ranges.append(start..<end)
                used = end == at ? 0 : used - (afterSpace?.used ?? 0)
                start = end
                afterSpace = nil
            }
            at += length
            used += width
            total += width
            if space { afterSpace = (at, used) }
            return true
        }
        var more = false
        let utf8 = text.utf8
        if utf8.allSatisfy({ $0 < 0x80 }) {
            for b in utf8 where !step(1, width: 1, space: b == 32 || b == 9) {
                more = true
                break
            }
        } else {
            for c in text where !step(c.utf16.count, width: Self.wide(c) ? 2 : 1, space: c == " " || c == "\t") {
                more = true
                break
            }
        }
        if at > start || ranges.isEmpty { ranges.append(start..<at) }
        return (ranges, more)
    }

    /// East Asian wide characters and emoji, which SF Mono's fallbacks draw about two columns wide.
    static func wide(_ c: Character) -> Bool {
        guard let v = c.unicodeScalars.first?.value else { return false }
        return (0x1100...0x115F).contains(v) || (0x2E80...0xA4CF).contains(v) || (0xAC00...0xD7A3).contains(v)
            || (0xF900...0xFAFF).contains(v) || (0xFE30...0xFE4F).contains(v) || (0xFF00...0xFF60).contains(v)
            || (0xFFE0...0xFFE6).contains(v) || (0x1F300...0x1FAFF).contains(v) || (0x20000...0x3FFFD).contains(v)
    }
}
