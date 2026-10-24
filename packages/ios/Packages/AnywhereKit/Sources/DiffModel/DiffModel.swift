import Wire

/// One row of a diff as the views draw it: its side's line numbers, its text and the changed words in it.
public struct UnifiedLine: Hashable, Sendable, Identifiable {
    public enum Kind: UInt8, Hashable, Sendable {
        case hunk, context, added, deleted
    }

    /// The row's index in its diff, stable while the diff is.
    public var id: Int
    public var kind: Kind
    public var old: Int?
    public var new: Int?
    public var text: String
    /// Changed words as UTF-16 offsets into `text`, for `NSRange` and `AttributedString`.
    public var words: [Range<Int>]
    /// `text` was cut at 1024 characters.
    public var cut: Bool

    public init(id: Int, kind: Kind, old: Int? = nil, new: Int? = nil, text: String, words: [Range<Int>] = [], cut: Bool = false) {
        self.id = id
        self.kind = kind
        self.old = old
        self.new = new
        self.text = text
        self.words = words
        self.cut = cut
    }
}

/// A left and right row of the side-by-side view, as indices into the unified rows; a hunk or context row sits on both.
public struct SplitRow: Hashable, Sendable, Identifiable {
    public var id: Int
    public var left: Int?
    public var right: Int?
    public init(id: Int, left: Int?, right: Int?) {
        self.id = id
        self.left = left
        self.right = right
    }
}

/// What a diff screen shows in place of rows.
public enum DiffState: Hashable, Sendable {
    case rows
    case binary
    case tooLarge
    case image
    /// The file has no changes for this comparison.
    case unchanged
}

/// A git.diff reply made ready to draw. Build it off the main actor: its cost grows with the diff.
public struct DiffDocument: Hashable, Sendable {
    public var path: String
    public var oldPath: String?
    public var language: String
    public var added: Int
    public var removed: Int
    public var state: DiffState
    public var lines: [UnifiedLine]
    public var truncated: Truncated?
    /// Digits of the widest line number, so the gutter keeps one width.
    public var gutterDigits: Int
    /// The side-by-side pairing of `lines`.
    public var pairs: [SplitRow]
    /// The longest line in UTF-8 bytes, near enough its characters, so side-by-side columns share one width.
    public var widest: Int

    public init(_ diff: GitDiff) {
        path = diff.path
        oldPath = diff.oldPath
        language = diff.language
        added = diff.added
        removed = diff.removed
        truncated = diff.truncated
        lines = DiffRows.lines(diff.lines)
        state =
            if diff.image == true { .image } else if diff.binary == true { .binary } else if diff.tooLarge == true { .tooLarge } else if lines.isEmpty {
                .unchanged
            } else { .rows }
        gutterDigits = DiffRows.digits(lines)
        pairs = DiffRows.split(lines)
        widest = lines.reduce(0) { $1.kind == .hunk ? $0 : max($0, $1.text.utf8.count) }
    }
}

public enum DiffRows {
    /// git.diff's rows with their word ranges moved from UTF-8 bytes to UTF-16.
    public static func lines(_ rows: [DiffRow]) -> [UnifiedLine] {
        var out: [UnifiedLine] = []
        out.reserveCapacity(rows.count)
        for (i, r) in rows.enumerated() {
            let kind: UnifiedLine.Kind =
                switch r.k {
                case .hunk: .hunk
                case .added: .added
                case .deleted: .deleted
                default: .context
                }
            out.append(
                UnifiedLine(
                    id: i, kind: kind, old: r.o, new: r.n, text: r.t, words: r.w.map { utf16(r.t, utf8: $0) } ?? [], cut: r.cut ?? false))
        }
        return out
    }

    /// Byte ranges of `text` as UTF-16 ranges; any that fall outside it or inside a character are dropped.
    public static func utf16(_ text: String, utf8 ranges: [[Int]]) -> [Range<Int>] {
        let bytes = text.utf8
        var out: [Range<Int>] = []
        for r in ranges where r.count == 2 && 0 <= r[0] && r[0] <= r[1] && r[1] <= bytes.count {
            let lo = bytes.index(bytes.startIndex, offsetBy: r[0])
            let hi = bytes.index(lo, offsetBy: r[1] - r[0])
            guard lo.samePosition(in: text.unicodeScalars) != nil, hi.samePosition(in: text.unicodeScalars) != nil else { continue }
            let start = text.utf16.distance(from: text.utf16.startIndex, to: lo)
            out.append(start..<(start + text.utf16.distance(from: lo, to: hi)))
        }
        return out
    }

    /// Pairs each run of deletions with the additions that follow it, side by side (git.rs split).
    public static func split(_ lines: [UnifiedLine]) -> [SplitRow] {
        var out: [SplitRow] = []
        out.reserveCapacity(lines.count)
        var i = 0
        while i < lines.count {
            if lines[i].kind == .hunk || lines[i].kind == .context {
                out.append(SplitRow(id: out.count, left: i, right: i))
                i += 1
                continue
            }
            var dels = 0
            while i + dels < lines.count, lines[i + dels].kind == .deleted { dels += 1 }
            var adds = 0
            while i + dels + adds < lines.count, lines[i + dels + adds].kind == .added { adds += 1 }
            for k in 0..<max(dels, adds) {
                out.append(SplitRow(id: out.count, left: k < dels ? i + k : nil, right: k < adds ? i + dels + k : nil))
            }
            i += dels + adds
        }
        return out
    }

    static func digits(_ lines: [UnifiedLine]) -> Int {
        let widest = lines.reduce(0) { max($0, $1.old ?? 0, $1.new ?? 0) }
        return max(String(widest).count, 2)
    }
}
