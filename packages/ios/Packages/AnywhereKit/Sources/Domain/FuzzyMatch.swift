/// Go-to-file matching: the query's letters in order anywhere in a path, scored as pocketd's `fsview.Score` scores
/// them, so the phone can narrow results as the reader types and mark the letters that matched.
public enum FuzzyMatch {
    static let match = 16, boundary = 10, segment = 6, camel = 8, consecutive = 10, basename = 12, exactName = 40
    static let gapStart = 3, gap = 1

    /// The query as matched: lowercased, without spaces.
    public static func normalize(_ query: String) -> [UInt8] {
        Array(query.lowercased().utf8.filter { $0 != 0x20 && $0 != 0x09 && $0 != 0x0a })
    }

    /// The score of `query` (normalized) in `path`, or nil when its letters aren't all there in order.
    public static func score(_ query: [UInt8], in path: String) -> Int? {
        let original = Array(path.utf8), lower = Array(path.lowercased().utf8)
        return score(query, original: original, lower: lower)?.score
    }

    /// The UTF-8 offsets of `path` the query's letters matched, for highlighting.
    public static func positions(_ query: [UInt8], in path: String) -> [Int] {
        let original = Array(path.utf8), lower = Array(path.lowercased().utf8)
        return score(query, original: original, lower: lower, positions: true)?.positions ?? []
    }

    /// Takes the rightmost match, which lands in the base name when it can, then tightens it from the left.
    static func score(_ query: [UInt8], original: [UInt8], lower: [UInt8], positions wanted: Bool = false) -> (score: Int, positions: [Int])? {
        if query.isEmpty { return (0, []) }
        if query.count > lower.count { return nil }
        return query.withUnsafeBufferPointer { q in
            lower.withUnsafeBufferPointer { l in
                original.withUnsafeBufferPointer { o in scan(q, o, l, wanted) }
            }
        }
    }

    private static func scan(_ q: UnsafeBufferPointer<UInt8>, _ o: UnsafeBufferPointer<UInt8>, _ l: UnsafeBufferPointer<UInt8>, _ wanted: Bool)
        -> (score: Int, positions: [Int])?
    {
        var qi = q.count - 1, start = -1
        var i = l.count - 1
        while i >= 0 {
            if l[i] == q[qi] {
                if qi == 0 {
                    start = i
                    break
                }
                qi -= 1
            }
            i -= 1
        }
        if start < 0 { return nil }
        let camelOK = o.count == l.count
        var name = 0
        if let slash = l.lastIndex(of: 0x2f) { name = slash + 1 }
        var score = 0, prev = -2
        var positions: [Int] = []
        if wanted { positions.reserveCapacity(q.count) }
        qi = 0
        i = start
        while i < l.count && qi < q.count {
            if l[i] == q[qi] {
                score += match
                if i == 0 || l[i - 1] == 0x2f {
                    score += boundary + segment
                } else if isSeparator(l[i - 1]) {
                    score += boundary
                } else if camelOK && isUpper(o[i]) && !isUpper(o[i - 1]) {
                    score += camel
                }
                if i == prev + 1 {
                    score += consecutive
                } else if prev >= 0 {
                    score -= gapStart + gap * (i - prev - 1)
                }
                if i >= name { score += basename }
                if wanted { positions.append(i) }
                prev = i
                qi += 1
            }
            i += 1
        }
        if exactName(q, l, from: name) { score += exactName }
        return (score, positions)
    }

    /// The base name is the query, with or without its extension.
    private static func exactName(_ q: UnsafeBufferPointer<UInt8>, _ l: UnsafeBufferPointer<UInt8>, from name: Int) -> Bool {
        guard name < l.count else { return false }
        var end = l.count
        if l.count - name == q.count, zip(q, l[name...]).allSatisfy(==) { return true }
        if let dot = l[(name + 1)...].lastIndex(of: 0x2e) { end = dot }
        return end - name == q.count && zip(q, l[name..<end]).allSatisfy(==)
    }

    @inline(__always) static func isUpper(_ b: UInt8) -> Bool { b >= 0x41 && b <= 0x5a }
    @inline(__always) static func isSeparator(_ b: UInt8) -> Bool { b == 0x5f || b == 0x2d || b == 0x2e || b == 0x20 }
}

/// Paths ready to search many times: each kept with its UTF-8 and lowercase bytes.
public struct FuzzyIndex: Sendable {
    public let paths: [String]
    let original: [[UInt8]]
    let lower: [[UInt8]]

    public init(_ paths: [String]) {
        self.paths = paths
        original = paths.map { Array($0.utf8) }
        lower = paths.map { Array($0.lowercased().utf8) }
    }

    /// The best `limit` paths for `query`: best score first, then `changed` ones, then shorter, then in the given order.
    public func search(_ query: String, limit: Int, changed: (String) -> Bool = { _ in false }) -> [String] {
        guard limit > 0 else { return [] }
        let q = FuzzyMatch.normalize(query)
        struct Hit { var score, unchanged, length, at: Int }
        func before(_ a: Hit, _ b: Hit) -> Bool {
            if a.score != b.score { return a.score > b.score }
            if a.unchanged != b.unchanged { return a.unchanged < b.unchanged }
            if a.length != b.length { return a.length < b.length }
            return a.at < b.at
        }
        var best: [Hit] = []
        best.reserveCapacity(limit + 1)
        for at in paths.indices {
            guard let s = FuzzyMatch.score(q, original: original[at], lower: lower[at]) else { continue }
            let hit = Hit(score: s.score, unchanged: changed(paths[at]) ? 0 : 1, length: q.isEmpty ? 0 : lower[at].count, at: at)
            if best.count == limit, !before(hit, best[limit - 1]) { continue }
            var lo = 0, hi = best.count
            while lo < hi {
                let mid = (lo + hi) / 2
                if before(best[mid], hit) { lo = mid + 1 } else { hi = mid }
            }
            best.insert(hit, at: lo)
            if best.count > limit { best.removeLast() }
        }
        return best.map { paths[$0.at] }
    }
}
