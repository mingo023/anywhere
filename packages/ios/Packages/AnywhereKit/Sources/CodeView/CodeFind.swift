import Foundation

/// Where a query appears in some lines, ignoring case, and which match is current.
public struct CodeFind: Sendable, Equatable {
    public struct Match: Hashable, Sendable {
        public let line: Int
        /// UTF-16 offsets in the line.
        public let range: Range<Int>
    }

    public let query: String
    public let matches: [Match]
    public private(set) var current = 0
    private let byLine: [Int: [Range<Int>]]

    public init(_ query: String, in lines: [String]) {
        self.query = query
        var matches: [Match] = []
        if !query.isEmpty {
            for (i, line) in lines.enumerated() {
                var from = line.startIndex
                while let r = line.range(of: query, options: [.caseInsensitive, .diacriticInsensitive], range: from..<line.endIndex) {
                    let lo = line.utf16.distance(from: line.startIndex, to: r.lowerBound)
                    matches.append(Match(line: i, range: lo..<lo + line.utf16.distance(from: r.lowerBound, to: r.upperBound)))
                    from = r.upperBound
                }
            }
        }
        self.matches = matches
        byLine = Dictionary(grouping: matches, by: \.line).mapValues { $0.map(\.range) }
    }

    public var match: Match? { matches.isEmpty ? nil : matches[current] }

    /// "3 of 12", or "No matches" once there is a query.
    public var position: String {
        if query.isEmpty { return "" }
        return matches.isEmpty ? "No matches" : "\(current + 1) of \(matches.count)"
    }

    /// Moves to the next match, or with `by: -1` the previous one, wrapping at the ends.
    public mutating func step(by: Int = 1) {
        guard !matches.isEmpty else { return }
        current = ((current + by) % matches.count + matches.count) % matches.count
    }

    public func ranges(in line: Int) -> [Range<Int>] { byLine[line] ?? [] }
}
