public struct GhState: OpenString {
    public let rawValue: String
    public init(rawValue: String) { self.rawValue = rawValue }
    public static let missing: Self = "missing"
    public static let signedOut: Self = "signedOut"
    public static let ready: Self = "ready"
}

public struct GhStatus: Codable, Hashable, Sendable {
    public var state: GhState
    public var login: String?
    public init(state: GhState, login: String? = nil) {
        self.state = state
        self.login = login
    }
}

public struct Checks: Codable, Hashable, Sendable {
    public var passed: Int
    public var failed: Int
    public var pending: Int
    public init(passed: Int, failed: Int, pending: Int) {
        self.passed = passed
        self.failed = failed
        self.pending = pending
    }
}

public struct CheckRun: Codable, Hashable, Sendable {
    public var name: String
    /// passed, failed, pending or skipped.
    public var outcome: String
    public var url: String?
    public init(name: String, outcome: String, url: String? = nil) {
        self.name = name
        self.outcome = outcome
        self.url = url
    }
}

public struct Review: Codable, Hashable, Sendable {
    public var login: String
    public var state: String
    public init(login: String, state: String) {
        self.login = login
        self.state = state
    }
}

public struct ThreadComment: Codable, Hashable, Sendable {
    public var author: String
    public var body: String
    /// Unix seconds.
    public var at: Int
    public var hunk: String?
    public init(author: String, body: String, at: Int, hunk: String? = nil) {
        self.author = author
        self.body = body
        self.at = at
        self.hunk = hunk
    }
}

public struct ReviewThread: Codable, Hashable, Sendable, Identifiable {
    public var id: String
    public var path: String
    public var line: Int?
    /// On the old side of the diff.
    public var left: Bool?
    public var outdated: Bool
    public var resolved: Bool
    public var comments: [ThreadComment]
    public init(id: String, path: String, line: Int? = nil, left: Bool? = nil, outdated: Bool, resolved: Bool, comments: [ThreadComment]) {
        self.id = id
        self.path = path
        self.line = line
        self.left = left
        self.outdated = outdated
        self.resolved = resolved
        self.comments = comments
    }
}

/// Where a PR stands, in the order `Pr::gate` decides it (packages/desktop/crates/git/src/github.rs).
public struct Gate: OpenString {
    public let rawValue: String
    public init(rawValue: String) { self.rawValue = rawValue }
    public static let done: Self = "done"
    public static let draft: Self = "draft"
    public static let conflicts: Self = "conflicts"
    public static let checksFailed: Self = "checksFailed"
    public static let checksRunning: Self = "checksRunning"
    public static let changesRequested: Self = "changesRequested"
    public static let reviewRequired: Self = "reviewRequired"
    public static let checking: Self = "checking"
    public static let blocked: Self = "blocked"
    public static let ready: Self = "ready"
}

public struct PullRequest: Codable, Hashable, Sendable {
    public var number: Int
    public var title: String
    public var url: String
    /// OPEN, CLOSED or MERGED.
    public var state: String
    public var draft: Bool
    public var base: String
    public var head: String
    public var checks: Checks
    public var runs: [CheckRun]
    /// APPROVED, CHANGES_REQUESTED or REVIEW_REQUIRED.
    public var review: String?
    public var reviews: [Review]
    public var requested: [String]
    /// MERGEABLE, CONFLICTING or UNKNOWN.
    public var mergeable: String?
    public var gate: Gate
    public var threads: [ReviewThread]

    public init(
        number: Int, title: String, url: String, state: String, draft: Bool, base: String, head: String, checks: Checks, runs: [CheckRun],
        review: String? = nil, reviews: [Review], requested: [String], mergeable: String? = nil, gate: Gate, threads: [ReviewThread]
    ) {
        self.number = number
        self.title = title
        self.url = url
        self.state = state
        self.draft = draft
        self.base = base
        self.head = head
        self.checks = checks
        self.runs = runs
        self.review = review
        self.reviews = reviews
        self.requested = requested
        self.mergeable = mergeable
        self.gate = gate
        self.threads = threads
    }
}

/// A tree's PR. No `pr` and no `problem` means the branch has none.
public struct GhPr: Codable, Hashable, Sendable {
    public var tree: String
    /// missing or signedOut.
    public var problem: GhState?
    public var pr: PullRequest?
    public init(tree: String, problem: GhState? = nil, pr: PullRequest? = nil) {
        self.tree = tree
        self.problem = problem
        self.pr = pr
    }
}

public struct PrItem: Codable, Hashable, Sendable {
    public var tree: String
    public var number: Int
    public var title: String
    public var state: String
    public var draft: Bool
    public var gate: Gate
    public var checks: Checks
    public init(tree: String, number: Int, title: String, state: String, draft: Bool, gate: Gate, checks: Checks) {
        self.tree = tree
        self.number = number
        self.title = title
        self.state = state
        self.draft = draft
        self.gate = gate
        self.checks = checks
    }
}

public struct GhPrs: Codable, Hashable, Sendable {
    public var items: [PrItem]
    public init(items: [PrItem]) { self.items = items }
}

public struct GhPrCreated: Codable, Hashable, Sendable {
    public var number: Int
    public var url: String
    public init(number: Int, url: String) {
        self.number = number
        self.url = url
    }
}

extension ClientMessage {
    public struct GhStatus: Codable, Hashable, Sendable {
        public var tree: String?
        public init(tree: String? = nil) { self.tree = tree }
    }

    public struct GhPrs: Codable, Hashable, Sendable {
        public var project: String
        public init(project: String) { self.project = project }
    }

    public struct GhPrCreate: Codable, Hashable, Sendable {
        public var tree: String
        public var title: String
        public var body: String
        public var draft: Bool?
        public var base: String?
        public init(tree: String, title: String, body: String, draft: Bool? = nil, base: String? = nil) {
            self.tree = tree
            self.title = title
            self.body = body
            self.draft = draft
            self.base = base
        }
    }

    public struct GhPrMerge: Codable, Hashable, Sendable {
        public var tree: String
        public var number: Int
        /// squash, merge or rebase.
        public var method: String
        public init(tree: String, number: Int, method: String) {
            self.tree = tree
            self.number = number
            self.method = method
        }
    }

    public struct GhPrReady: Codable, Hashable, Sendable {
        public var tree: String
        public var number: Int
        public init(tree: String, number: Int) {
            self.tree = tree
            self.number = number
        }
    }

    public struct GhThreadReply: Codable, Hashable, Sendable {
        public var tree: String
        public var thread: String
        public var body: String
        public init(tree: String, thread: String, body: String) {
            self.tree = tree
            self.thread = thread
            self.body = body
        }
    }

    public struct GhThreadResolve: Codable, Hashable, Sendable {
        public var tree: String
        public var thread: String
        public var resolved: Bool
        public init(tree: String, thread: String, resolved: Bool) {
            self.tree = tree
            self.thread = thread
            self.resolved = resolved
        }
    }
}
