public struct Worktree: Codable, Hashable, Sendable {
    public var name: String
    public var path: String
    /// Empty on a detached head.
    public var branch: String
    public var isMain: Bool

    public init(name: String, path: String, branch: String, isMain: Bool) {
        self.name = name
        self.path = path
        self.branch = branch
        self.isMain = isMain
    }
}

public struct Project: Codable, Hashable, Sendable, Identifiable {
    public var path: String
    public var name: String
    public var worktrees: [Worktree]

    public var id: String { path }

    public init(path: String, name: String, worktrees: [Worktree]) {
        self.path = path
        self.name = name
        self.worktrees = worktrees
    }
}

/// Every project: the reply to `project.list`, and pushed whole to workspace.v1 peers on each change.
public struct ProjectList: Codable, Hashable, Sendable {
    public var projects: [Project]
    public init(projects: [Project]) { self.projects = projects }
}

/// Every worktree's display name by path, sent whole on hello and on each change.
public struct WorktreeNames: Codable, Hashable, Sendable {
    public var names: [String: String]
    public init(names: [String: String]) { self.names = names }
}

public struct WorktreeCreated: Codable, Hashable, Sendable {
    public var requestId: String
    public var path: String
    public var branch: String
    public init(requestId: String, path: String, branch: String) {
        self.requestId = requestId
        self.path = path
        self.branch = branch
    }
}

public struct Branch: Codable, Hashable, Sendable {
    public var name: String
    /// Unix seconds of its tip commit.
    public var at: Int
    public init(name: String, at: Int) {
        self.name = name
        self.at = at
    }
}

/// Newest first, at most 200 each.
public struct BranchList: Codable, Hashable, Sendable {
    public var current: String?
    public var local: [Branch]
    public var remote: [Branch]
    public init(current: String? = nil, local: [Branch], remote: [Branch]) {
        self.current = current
        self.local = local
        self.remote = remote
    }
}

public struct Usage: Codable, Hashable, Sendable {
    /// 0 to 100.
    public var pct: Double
    /// Unix seconds.
    public var resetsAt: Int?
    public init(pct: Double, resetsAt: Int? = nil) {
        self.pct = pct
        self.resetsAt = resetsAt
    }
}

public struct Account: Codable, Hashable, Sendable, Identifiable {
    public var id: String
    public var provider: String
    public var name: String
    public var plan: String?
    public var email: String?
    public var signedIn: Bool
    public var isDefault: Bool?
    public var fiveHour: Usage?
    public var week: Usage?

    public init(
        id: String, provider: String, name: String, plan: String? = nil, email: String? = nil, signedIn: Bool, isDefault: Bool? = nil,
        fiveHour: Usage? = nil, week: Usage? = nil
    ) {
        self.id = id
        self.provider = provider
        self.name = name
        self.plan = plan
        self.email = email
        self.signedIn = signedIn
        self.isDefault = isDefault
        self.fiveHour = fiveHour
        self.week = week
    }
}

public struct AccountList: Codable, Hashable, Sendable {
    public var accounts: [Account]
    public init(accounts: [Account]) { self.accounts = accounts }
}

extension ClientMessage {
    public struct WorktreeRename: Codable, Hashable, Sendable {
        public var path: String
        public var title: String
        public init(path: String, title: String) {
            self.path = path
            self.title = title
        }
    }

    /// At most one of `base`, `branch` and `pr`.
    public struct WorktreeCreate: Codable, Hashable, Sendable {
        public var requestId: String
        public var project: String
        public var name: String
        public var base: String?
        public var branch: String?
        public var pr: Int?
        public var setup: Bool?

        public init(requestId: String, project: String, name: String, base: String? = nil, branch: String? = nil, pr: Int? = nil, setup: Bool? = nil) {
            self.requestId = requestId
            self.project = project
            self.name = name
            self.base = base
            self.branch = branch
            self.pr = pr
            self.setup = setup
        }
    }

    public struct WorktreeDelete: Codable, Hashable, Sendable {
        public var path: String
        public var deleteBranch: Bool?
        public init(path: String, deleteBranch: Bool? = nil) {
            self.path = path
            self.deleteBranch = deleteBranch
        }
    }

    /// A verb about one tree: `branch.list`, `gh.pr`, `git.push` without a lease, `git.pull`.
    public struct TreeRef: Codable, Hashable, Sendable {
        public var tree: String
        public init(tree: String) { self.tree = tree }
    }

    public struct AccountList: Codable, Hashable, Sendable {
        public var provider: String?
        public init(provider: String? = nil) { self.provider = provider }
    }
}
