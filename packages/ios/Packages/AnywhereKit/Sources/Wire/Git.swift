/// A file's git status letter: "M" "A" "D" "R" "U" (conflict) or "?" (untracked).
public struct FileStatus: OpenString {
    public let rawValue: String
    public init(rawValue: String) { self.rawValue = rawValue }
    public static let modified: Self = "M"
    public static let added: Self = "A"
    public static let deleted: Self = "D"
    public static let renamed: Self = "R"
    public static let conflict: Self = "U"
    public static let untracked: Self = "?"
}

public struct GitTotals: Codable, Hashable, Sendable {
    public var files: Int
    public var added: Int
    public var removed: Int
    public var staged: Int
    public init(files: Int, added: Int, removed: Int, staged: Int) {
        self.files = files
        self.added = added
        self.removed = removed
        self.staged = staged
    }
}

public struct GitFile: Codable, Hashable, Sendable, Identifiable {
    public var path: String
    public var oldPath: String?
    public var status: FileStatus
    public var staged: Bool
    public var unstaged: Bool
    /// Omitted for untracked files.
    public var added: Int?
    public var removed: Int?
    public var binary: Bool?

    public var id: String { path }

    public init(path: String, oldPath: String? = nil, status: FileStatus, staged: Bool, unstaged: Bool, added: Int? = nil, removed: Int? = nil, binary: Bool? = nil) {
        self.path = path
        self.oldPath = oldPath
        self.status = status
        self.staged = staged
        self.unstaged = unstaged
        self.added = added
        self.removed = removed
        self.binary = binary
    }
}

public struct CommitRef: Codable, Hashable, Sendable {
    public var sha: String
    public var subject: String
    public init(sha: String, subject: String) {
        self.sha = sha
        self.subject = subject
    }
}

/// One page of a tree's changes, files sorted by path.
public struct GitStatus: Codable, Hashable, Sendable {
    public var tree: String
    /// An opaque stamp of the porcelain output.
    public var rev: String
    public var branch: String?
    public var detached: Bool?
    public var upstream: String?
    public var base: String?
    public var ahead: Int
    public var behind: Int
    /// Commits not on the upstream; none before the first push.
    public var unpushed: Int?
    public var totals: GitTotals
    public var files: [GitFile]
    /// At most 20 since the base.
    public var commits: [CommitRef]
    /// The file offset of the next page.
    public var nextCursor: Int?

    public init(
        tree: String, rev: String, branch: String? = nil, detached: Bool? = nil, upstream: String? = nil, base: String? = nil, ahead: Int,
        behind: Int, unpushed: Int? = nil, totals: GitTotals, files: [GitFile], commits: [CommitRef], nextCursor: Int? = nil
    ) {
        self.tree = tree
        self.rev = rev
        self.branch = branch
        self.detached = detached
        self.upstream = upstream
        self.base = base
        self.ahead = ahead
        self.behind = behind
        self.unpushed = unpushed
        self.totals = totals
        self.files = files
        self.commits = commits
        self.nextCursor = nextCursor
    }
}

/// A watched tree's rev moved.
public struct GitChanged: Codable, Hashable, Sendable {
    public var tree: String
    public var rev: String
    public init(tree: String, rev: String) {
        self.tree = tree
        self.rev = rev
    }
}

/// A watched tree's PR result changed.
public struct GhChanged: Codable, Hashable, Sendable {
    public var tree: String
    public init(tree: String) { self.tree = tree }
}

/// What a diff compares.
public enum DiffAt: Codable, Hashable, Sendable {
    case working
    case staged
    case commit(rev: String)
    /// Against the branch's base, or `rev` when given.
    case base(rev: String?)

    enum Keys: String, CodingKey {
        case kind, rev
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: Keys.self)
        switch try c.decode(String.self, forKey: .kind) {
        case "working": self = .working
        case "staged": self = .staged
        case "commit": self = .commit(rev: try c.decode(String.self, forKey: .rev))
        case "base": self = .base(rev: try c.decodeIfPresent(String.self, forKey: .rev))
        case let kind: throw malformed(decoder, "unknown diff kind \(kind)")
        }
    }

    public func encode(to encoder: any Encoder) throws {
        var c = encoder.container(keyedBy: Keys.self)
        switch self {
        case .working: try c.encode("working", forKey: .kind)
        case .staged: try c.encode("staged", forKey: .kind)
        case .commit(let rev):
            try c.encode("commit", forKey: .kind)
            try c.encode(rev, forKey: .rev)
        case .base(let rev):
            try c.encode("base", forKey: .kind)
            try c.encodeIfPresent(rev, forKey: .rev)
        }
    }
}

/// One row of a diff, with one-letter keys because a diff can hold thousands.
public struct DiffRow: Codable, Hashable, Sendable {
    public struct Kind: OpenString {
        public let rawValue: String
        public init(rawValue: String) { self.rawValue = rawValue }
        public static let hunk: Self = "h"
        public static let context: Self = "c"
        public static let added: Self = "a"
        public static let deleted: Self = "d"
    }

    public var k: Kind
    /// Old line number.
    public var o: Int?
    /// New line number.
    public var n: Int?
    public var t: String
    /// Word-level changed ranges, `[start, end]` as UTF-8 byte offsets into `t`.
    public var w: [[Int]]?
    /// `t` was cut at 1024 characters.
    public var cut: Bool?

    public init(k: Kind, o: Int? = nil, n: Int? = nil, t: String, w: [[Int]]? = nil, cut: Bool? = nil) {
        self.k = k
        self.o = o
        self.n = n
        self.t = t
        self.w = w
        self.cut = cut
    }
}

public struct Truncated: Codable, Hashable, Sendable {
    public var shown: Int
    public var total: Int
    public init(shown: Int, total: Int) {
        self.shown = shown
        self.total = total
    }
}

public struct GitDiff: Codable, Hashable, Sendable {
    public var tree: String
    public var path: String
    public var oldPath: String?
    /// rust, typescript, tsx, javascript, python, go, swift, kotlin, java, json, markdown, toml, yaml, bash, css, html or text.
    public var language: String
    public var added: Int
    public var removed: Int
    public var binary: Bool?
    public var tooLarge: Bool?
    public var image: Bool?
    public var lines: [DiffRow]
    public var truncated: Truncated?

    public init(
        tree: String, path: String, oldPath: String? = nil, language: String, added: Int, removed: Int, binary: Bool? = nil,
        tooLarge: Bool? = nil, image: Bool? = nil, lines: [DiffRow], truncated: Truncated? = nil
    ) {
        self.tree = tree
        self.path = path
        self.oldPath = oldPath
        self.language = language
        self.added = added
        self.removed = removed
        self.binary = binary
        self.tooLarge = tooLarge
        self.image = image
        self.lines = lines
        self.truncated = truncated
    }
}

public struct LogTips: Codable, Hashable, Sendable {
    public var head: String?
    public var branch: String?
    public var upstream: String?
    public var base: String?
    public init(head: String? = nil, branch: String? = nil, upstream: String? = nil, base: String? = nil) {
        self.head = head
        self.branch = branch
        self.upstream = upstream
        self.base = base
    }
}

public struct LogCommit: Codable, Hashable, Sendable, Identifiable {
    public var sha: String
    public var parents: [String]
    public var author: String
    /// Unix seconds.
    public var at: Int
    public var subject: String

    public var id: String { sha }

    public init(sha: String, parents: [String], author: String, at: Int, subject: String) {
        self.sha = sha
        self.parents = parents
        self.author = author
        self.at = at
        self.subject = subject
    }
}

public struct GitLog: Codable, Hashable, Sendable {
    public var tips: LogTips
    public var commits: [LogCommit]
    public var more: Bool
    public init(tips: LogTips, commits: [LogCommit], more: Bool) {
        self.tips = tips
        self.commits = commits
        self.more = more
    }
}

public struct CommitFile: Codable, Hashable, Sendable {
    public var path: String
    public var oldPath: String?
    public var status: FileStatus
    public var added: Int?
    public var removed: Int?
    public init(path: String, oldPath: String? = nil, status: FileStatus, added: Int? = nil, removed: Int? = nil) {
        self.path = path
        self.oldPath = oldPath
        self.status = status
        self.added = added
        self.removed = removed
    }
}

/// The reply to `git.commit`: one commit and the files it touched.
public struct CommitDetail: Codable, Hashable, Sendable {
    public var sha: String
    public var subject: String
    public var body: String?
    public var author: String
    /// Unix seconds.
    public var at: Int
    public var files: [CommitFile]
    public init(sha: String, subject: String, body: String? = nil, author: String, at: Int, files: [CommitFile]) {
        self.sha = sha
        self.subject = subject
        self.body = body
        self.author = author
        self.at = at
        self.files = files
    }
}

public struct GitCommitted: Codable, Hashable, Sendable {
    public var sha: String
    public init(sha: String) { self.sha = sha }
}

extension ClientMessage {
    public struct GitStatus: Codable, Hashable, Sendable {
        public var tree: String
        public var cursor: Int?
        /// Default 500, at most 2000.
        public var limit: Int?
        public init(tree: String, cursor: Int? = nil, limit: Int? = nil) {
            self.tree = tree
            self.cursor = cursor
            self.limit = limit
        }
    }

    /// Replaces this connection's watch set; none stops.
    public struct GitWatch: Codable, Hashable, Sendable {
        public var trees: [String]
        public init(trees: [String]) { self.trees = trees }
    }

    public struct GitDiff: Codable, Hashable, Sendable {
        public var tree: String
        public var path: String
        public var at: DiffAt
        public var context: Int?
        public var ignoreWhitespace: Bool?
        public var maxLines: Int?
        public init(tree: String, path: String, at: DiffAt, context: Int? = nil, ignoreWhitespace: Bool? = nil, maxLines: Int? = nil) {
            self.tree = tree
            self.path = path
            self.at = at
            self.context = context
            self.ignoreWhitespace = ignoreWhitespace
            self.maxLines = maxLines
        }
    }

    public struct GitLog: Codable, Hashable, Sendable {
        public var tree: String
        public var all: Bool?
        public var skip: Int?
        /// Default 50, at most 200.
        public var limit: Int?
        public init(tree: String, all: Bool? = nil, skip: Int? = nil, limit: Int? = nil) {
            self.tree = tree
            self.all = all
            self.skip = skip
            self.limit = limit
        }
    }

    public struct GitCommit: Codable, Hashable, Sendable {
        public var tree: String
        public var sha: String
        public init(tree: String, sha: String) {
            self.tree = tree
            self.sha = sha
        }
    }

    public struct GitStage: Codable, Hashable, Sendable {
        public var tree: String
        public var paths: [String]
        public var staged: Bool
        public init(tree: String, paths: [String], staged: Bool) {
            self.tree = tree
            self.paths = paths
            self.staged = staged
        }
    }

    public struct GitDiscard: Codable, Hashable, Sendable {
        public var tree: String
        public var paths: [String]
        public init(tree: String, paths: [String]) {
            self.tree = tree
            self.paths = paths
        }
    }

    public struct GitCommitCreate: Codable, Hashable, Sendable {
        public var tree: String
        public var message: String
        public var amend: Bool?
        public var all: Bool?
        public init(tree: String, message: String, amend: Bool? = nil, all: Bool? = nil) {
            self.tree = tree
            self.message = message
            self.amend = amend
            self.all = all
        }
    }

    public struct GitPush: Codable, Hashable, Sendable {
        public var tree: String
        public var lease: Bool?
        public init(tree: String, lease: Bool? = nil) {
            self.tree = tree
            self.lease = lease
        }
    }
}
